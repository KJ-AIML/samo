//! The host's limits on a helper's output (R8): bounded lines, a capped log, and a coalesced UI wake.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

/// R8: the longest line a helper may send.
pub const MAX_LINE: usize = 64 * 1024;
/// R8: `helper.log` is rotated once past this.
pub const MAX_LOG: u64 = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    Text(String),
    /// Longer than `max`; it was read to its end and dropped, never buffered.
    TooLong(usize),
}

/// Reads one line of at most `max` bytes. `Ok(None)` at the end of the stream. An overlong line is consumed and
/// dropped without holding more than `max` bytes.
pub fn read_line_capped(reader: &mut impl BufRead, max: usize) -> io::Result<Option<Line>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut seen = 0usize;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(if seen == 0 {
                None
            } else if seen > max {
                Some(Line::TooLong(seen))
            } else {
                Some(Line::Text(String::from_utf8_lossy(&buf).into_owned()))
            });
        }
        let (part, done) = match chunk.iter().position(|b| *b == b'\n') {
            Some(i) => (&chunk[..i], Some(i + 1)),
            None => (chunk, None),
        };
        seen = seen.saturating_add(part.len());
        if seen <= max {
            buf.extend_from_slice(part);
        } else {
            buf = Vec::new();
        }
        let used = done.unwrap_or(chunk.len());
        reader.consume(used);
        if done.is_some() {
            return Ok(Some(if seen > max {
                Line::TooLong(seen)
            } else {
                Line::Text(String::from_utf8_lossy(&buf).into_owned())
            }));
        }
    }
}

/// `data/helper.log`: the helper's stderr and the host's notes about it, rotated once past 1 MiB.
#[derive(Clone)]
pub struct HelperLog {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl HelperLog {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("helper.log"),
            lock: Arc::new(Mutex::new(())),
        }
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn note(&self, line: &str) {
        let Ok(_guard) = self.lock.lock() else {
            return;
        };
        if fs::metadata(&self.path).is_ok_and(|m| m.len() > MAX_LOG) {
            let _ = fs::rename(&self.path, self.path.with_extension("log.1"));
        }
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "{stamp} {line}");
        }
    }
}

struct WakeState {
    dirty: bool,
    closed: bool,
}

/// Calls the UI's wake at most once per `every`, and always once after the last news (R8: at most 4 a second).
#[derive(Clone)]
pub struct Waker {
    shared: Arc<(Mutex<WakeState>, Condvar)>,
}

impl Waker {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>, every: Duration) -> Self {
        let shared = Arc::new((
            Mutex::new(WakeState {
                dirty: false,
                closed: false,
            }),
            Condvar::new(),
        ));
        let thread_shared = Arc::clone(&shared);
        let _ = thread::Builder::new()
            .name("samo-ext-wake".into())
            .spawn(move || {
                let (lock, cvar) = &*thread_shared;
                loop {
                    {
                        let Ok(mut state) = lock.lock() else {
                            return;
                        };
                        while !state.dirty && !state.closed {
                            state = match cvar.wait(state) {
                                Ok(s) => s,
                                Err(_) => return,
                            };
                        }
                        if state.closed {
                            return;
                        }
                        state.dirty = false;
                    }
                    wake();
                    thread::sleep(every);
                }
            });
        Self { shared }
    }

    pub fn poke(&self) {
        let (lock, cvar) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.dirty = true;
            cvar.notify_one();
        }
    }

    pub fn close(&self) {
        let (lock, cvar) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.closed = true;
            cvar.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn capped_lines() -> Result<(), String> {
        let long = "x".repeat(100);
        let input = format!("short\n{long}\nlast-no-newline");
        let mut reader = BufReader::with_capacity(8, input.as_bytes());
        let read = |r: &mut BufReader<&[u8]>| read_line_capped(r, 10).map_err(|e| e.to_string());
        assert_eq!(read(&mut reader)?, Some(Line::Text("short".into())));
        assert_eq!(read(&mut reader)?, Some(Line::TooLong(100)));
        assert_eq!(read(&mut reader)?, Some(Line::TooLong(15)));
        assert_eq!(read(&mut reader)?, None);

        let mut reader = BufReader::new("a\n\nb".as_bytes());
        assert_eq!(read(&mut reader)?, Some(Line::Text("a".into())));
        assert_eq!(read(&mut reader)?, Some(Line::Text(String::new())));
        assert_eq!(read(&mut reader)?, Some(Line::Text("b".into())));
        Ok(())
    }

    #[test]
    fn log_rotates_past_its_cap() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let log = HelperLog::new(dir.path());
        let big = "y".repeat(64 * 1024);
        for _ in 0..20 {
            log.note(&big);
        }
        let size = fs::metadata(log.path()).map_err(|e| e.to_string())?.len();
        assert!(size <= MAX_LOG + 70 * 1024, "{size}");
        assert!(dir.path().join("helper.log.1").is_file());
        Ok(())
    }

    #[test]
    fn wake_is_coalesced() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let waker = Waker::start(
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(250),
        );
        // 100 pokes over about half a second.
        for _ in 0..100 {
            waker.poke();
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(600));
        let n = calls.load(Ordering::SeqCst);
        // At most one per 250 ms, plus the trailing one after the last poke.
        assert!((1..=4).contains(&n), "{n} wakes");
        waker.close();
    }
}
