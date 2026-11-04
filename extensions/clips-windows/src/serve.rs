//! `samo-clips serve`: what Samo runs while Clips is on. It says hello, sends the page and the badge, looks at the
//! clipboard when Windows says it changed (twice a second if Windows won't say; one sequence-number read when
//! nothing changed, R21), and answers Samo's messages. It exits when Samo says `shutdown` or closes its stdin; the
//! history stays.

use crate::clips::Clips;
use crate::watcher::Board;
use chrono::Utc;
use samo_ext_protocol::{HelperMsg, HostMsg, PROTOCOL_VERSION, decode_host_line, encode};
use std::fs::{self, File, TryLockError};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

/// How often Clips looks at the clipboard when Windows won't say when it changes.
pub const EVERY: Duration = Duration::from_millis(500);
/// How often the page is refreshed when Windows does say (a pause ending shows within this).
pub const QUIET: Duration = Duration::from_secs(5);

/// What the serving loop waits for.
pub enum Event {
    Host(HostMsg),
    /// Samo closed Clips' stdin.
    Gone,
    /// Windows says the clipboard changed: look now.
    Changed,
}

fn send(out: &mut impl Write, msg: &HelperMsg) -> bool {
    let Ok(line) = encode(msg) else { return true };
    writeln!(out, "{line}").and_then(|()| out.flush()).is_ok()
}

/// Serves Samo until it says `shutdown` or goes: hello first, then the page and the badge, again whenever they
/// change. The history is saved when it stops.
pub fn serve<B: Board>(
    clips: &mut Clips<B>,
    events: &Receiver<Event>,
    out: &mut impl Write,
    every: Duration,
) {
    let hello = HelperMsg::Hello {
        protocol: PROTOCOL_VERSION,
    };
    let mut force = true;
    if send(out, &hello) {
        loop {
            let now = Utc::now();
            let mut said = Vec::new();
            if !force {
                match events.recv_timeout(every) {
                    Ok(Event::Host(HostMsg::Shutdown) | Event::Gone)
                    | Err(RecvTimeoutError::Disconnected) => break,
                    Ok(Event::Host(msg)) => said = clips.handle(&msg, now),
                    Ok(Event::Changed) | Err(RecvTimeoutError::Timeout) => {}
                }
                clips.tick(now);
            }
            said.extend(clips.updates(now, force));
            if !said.iter().all(|msg| send(out, msg)) {
                break;
            }
            force = false;
        }
    }
    clips.shutdown();
}

/// One Clips at a time, even with two Samos running: the lock while it is held, `None` if another copy holds it.
pub fn lock(dir: &Path) -> std::io::Result<Option<File>> {
    fs::create_dir_all(dir)?;
    let file = File::create(dir.join("serve.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// Clips' data folder: Samo gives it in `SAMO_EXT_DATA_DIR`.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SAMO_EXT_DATA_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}

/// `serve` on this process's stdin and stdout, with this OS's clipboard. `listen` starts telling of changes on the
/// channel it is given, and says whether it could.
pub fn run<B: Board>(
    board: B,
    front: Box<dyn Fn() -> Option<String> + Send>,
    listen: impl FnOnce(mpsc::Sender<Event>) -> bool,
) -> ExitCode {
    let Some(dir) = data_dir() else {
        eprintln!("samo-clips: Samo didn't say where Clips keeps its files (SAMO_EXT_DATA_DIR)");
        return ExitCode::FAILURE;
    };
    let mut stdout = std::io::stdout().lock();
    let held = match lock(&dir) {
        Ok(Some(held)) => held,
        Ok(None) => {
            send(
                &mut stdout,
                &HelperMsg::Hello {
                    protocol: PROTOCOL_VERSION,
                },
            );
            send(
                &mut stdout,
                &HelperMsg::Health {
                    ok: false,
                    message: "Clips is already running in another copy of Samo".into(),
                },
            );
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("samo-clips: can't lock {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };
    let (tx, rx) = mpsc::channel();
    let every = if listen(tx.clone()) { QUIET } else { EVERY };
    let _ = thread::Builder::new().name("stdin".into()).spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Ok(msg) = decode_host_line(&line)
                && tx.send(Event::Host(msg)).is_err()
            {
                return;
            }
        }
        let _ = tx.send(Event::Gone);
    });
    let mut clips = Clips::open(&dir, board, front);
    serve(&mut clips, &rx, &mut stdout, every);
    drop(held);
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBoard;
    use samo_ext_protocol::{Widget, decode_helper_line};
    use std::sync::mpsc;

    fn said(out: &[u8]) -> Vec<HelperMsg> {
        String::from_utf8_lossy(out)
            .lines()
            .filter_map(|l| decode_helper_line(l).ok())
            .collect()
    }

    fn served(dir: &std::path::Path, events: Vec<Event>) -> Vec<HelperMsg> {
        let mut clips = Clips::open(dir, FakeBoard::default(), Box::new(|| None));
        let (tx, rx) = mpsc::channel();
        for e in events {
            let _ = tx.send(e);
        }
        let mut out = Vec::new();
        serve(&mut clips, &rx, &mut out, Duration::from_millis(10));
        said(&out)
    }

    #[test]
    fn hello_comes_first_then_the_page_and_badge() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let msgs = served(tmp.path(), vec![Event::Gone]);
        assert!(matches!(
            msgs.first(),
            Some(HelperMsg::Hello {
                protocol: PROTOCOL_VERSION
            })
        ));
        assert!(matches!(msgs.get(1), Some(HelperMsg::View { .. })));
        assert!(matches!(msgs.get(2), Some(HelperMsg::Badge { .. })));
        Ok(())
    }

    #[test]
    fn a_change_windows_tells_of_is_kept_at_once() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut clips = Clips::open(tmp.path(), FakeBoard::default(), Box::new(|| None));
        clips.board_mut().copy_text("told");
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(Event::Changed);
        let _ = tx.send(Event::Gone);
        let mut out = Vec::new();
        // An hour between looks: only Windows' word makes it look.
        serve(&mut clips, &rx, &mut out, Duration::from_secs(3600));
        assert_eq!(clips.history().items().len(), 1);
        Ok(())
    }

    #[test]
    fn samos_messages_get_a_new_page() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let pause = HostMsg::Action {
            id: "pause".into(),
            row: None,
        };
        let msgs = served(tmp.path(), vec![Event::Host(pause), Event::Gone]);
        let resume_offered = msgs.iter().any(|m| match m {
            HelperMsg::View { sections } => sections
                .iter()
                .any(|w| matches!(w, Widget::Button { action, .. } if action == "resume")),
            _ => false,
        });
        assert!(resume_offered);
        Ok(())
    }

    #[test]
    fn shutdown_stops_serving() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        // Samo is still there (the channel stays open), but said shutdown.
        let mut clips = Clips::open(tmp.path(), FakeBoard::default(), Box::new(|| None));
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(Event::Host(HostMsg::Shutdown));
        let mut out = Vec::new();
        serve(&mut clips, &rx, &mut out, Duration::from_millis(10));
        assert!(tmp.path().join("history.json").is_file(), "not saved");
        drop(tx);
        Ok(())
    }

    /// A pipe Samo closed.
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_pipe_stops_serving() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut clips = Clips::open(tmp.path(), FakeBoard::default(), Box::new(|| None));
        let (_tx, rx) = mpsc::channel();
        serve(&mut clips, &rx, &mut Closed, Duration::from_millis(10));
        Ok(())
    }

    #[test]
    fn a_second_copy_says_clips_is_running_elsewhere() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let first = lock(tmp.path());
        assert!(matches!(first, Ok(Some(_))));
        let second = lock(tmp.path());
        assert!(matches!(second, Ok(None)), "two copies of Clips at once");
        drop(first);
        assert!(matches!(lock(tmp.path()), Ok(Some(_))));
        Ok(())
    }
}
