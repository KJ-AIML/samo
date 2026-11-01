//! One thread per running helper: it starts the helper, reads what it says within R8's limits, restarts it after
//! a crash (backing off), and stops it cleanly.

use crate::History;
use crate::host::Inner;
use crate::limits::{HelperLog, Line, MAX_LINE, read_line_capped};
use crate::state::{ExtSlot, PendingPopup, Status};
use samo_domain::extensions::{Backoff, RateGate, Restart};
use samo_ext_protocol::{
    HelperMsg, HostMsg, NoteKind, PROTOCOL_VERSION, Widget, decode_helper_line, encode, rows_in,
};
use std::env;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// R8: a page holds at most this many rows.
pub const MAX_ROWS: usize = 200;
/// How often the supervisor wakes when nothing happens, to flush held messages and check the helper.
const TICK: Duration = Duration::from_millis(250);

pub(crate) enum Event {
    /// A line from the helper's stdout, tagged with which run of the helper said it.
    Line(u64, Line),
    /// The helper's stdout closed.
    Eof(u64),
    /// Something for the helper.
    Send(HostMsg),
    /// Stop the helper and end.
    Stop,
}

pub(crate) struct Job {
    pub inner: Arc<Inner>,
    pub slot: ExtSlot,
    pub id: String,
    pub helper: PathBuf,
    pub data_dir: PathBuf,
    pub events: Sender<Event>,
}

enum Outcome {
    /// The host asked it to stop.
    Stopped,
    /// It can't go on; restarting won't help.
    Halt(String),
    /// It ended by itself; it may be restarted.
    Crashed(String),
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The only environment a helper gets.
fn helper_env(data_dir: &Path) -> Vec<(&'static str, String)> {
    let var = |name: &str, fallback: &str| {
        env::var(name)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    };
    vec![
        ("HOME", var("HOME", "/")),
        ("PATH", var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")),
        ("LANG", var("LANG", "en_US.UTF-8")),
        ("SAMO_EXT_DATA_DIR", data_dir.to_string_lossy().into_owned()),
    ]
}

/// Runs `<helper> <verb>` (`on`, `off`, `uninstall`) once, with the helper's environment, and waits for it up to
/// `timeout`. `Err` says what went wrong in words.
pub(crate) fn one_shot(
    helper: &Path,
    verb: &str,
    data_dir: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let _ = std::fs::create_dir_all(data_dir);
    let mut child = Command::new(helper)
        .arg(verb)
        .env_clear()
        .envs(helper_env(data_dir))
        .current_dir(data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("couldn't run `{verb}`: {e}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("`{verb}` failed ({status})")),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("`{verb}` took longer than {}s", timeout.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("couldn't wait for `{verb}`: {e}")),
        }
    }
}

pub(crate) fn run(job: Job, rx: Receiver<Event>) {
    let _ = std::fs::create_dir_all(&job.data_dir);
    let log = HelperLog::new(&job.data_dir);
    let mut backoff = Backoff::new(job.inner.config.backoff);
    let mut generation = 0u64;
    loop {
        generation += 1;
        match run_once(&job, &rx, &log, generation) {
            Outcome::Stopped => return,
            Outcome::Halt(reason) => {
                log.note(&format!("stopped: {reason}"));
                job.inner.update(job.slot, |rt| {
                    rt.status = Status::Stopped(reason);
                    rt.badge = 0;
                    rt.control = None;
                });
                return;
            }
            Outcome::Crashed(why) => {
                log.note(&format!("crashed: {why}"));
                match backoff.on_crash(now_ms()) {
                    Restart::Stop => {
                        let reason = format!(
                            "It crashed {} times in {} minutes. Last time: {why}",
                            job.inner.config.backoff.max_crashes,
                            job.inner.config.backoff.window_ms / 60_000
                        );
                        job.inner.update(job.slot, |rt| {
                            rt.status = Status::Stopped(reason);
                            rt.badge = 0;
                            rt.control = None;
                        });
                        return;
                    }
                    Restart::After(ms) => {
                        job.inner.update(job.slot, |rt| {
                            rt.status = Status::Starting;
                            rt.badge = 0;
                        });
                        let deadline = Instant::now() + Duration::from_millis(ms);
                        loop {
                            let left = deadline.saturating_duration_since(Instant::now());
                            if left.is_zero() {
                                break;
                            }
                            match rx.recv_timeout(left) {
                                Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                                // Lines from the run that crashed, or messages while it is down.
                                Ok(_) => {}
                                Err(RecvTimeoutError::Timeout) => break,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Messages held back by the rate limit; only the latest of each is kept (R8).
#[derive(Default)]
struct Held {
    view: Option<Vec<Widget>>,
    badge: Option<(u32, String)>,
    health: Option<(bool, String)>,
}

impl Held {
    fn is_empty(&self) -> bool {
        self.view.is_none() && self.badge.is_none() && self.health.is_none()
    }
}

struct Run<'a> {
    job: &'a Job,
    log: &'a HelperLog,
    popups: RateGate,
    /// The last unhealthy message: if the helper then exits, that's why, and it isn't restarted.
    unhealthy: Option<String>,
}

fn run_once(job: &Job, rx: &Receiver<Event>, log: &HelperLog, generation: u64) -> Outcome {
    let config = &job.inner.config;
    let mut child = match Command::new(&job.helper)
        .arg("serve")
        .env_clear()
        .envs(helper_env(&job.data_dir))
        .current_dir(&job.data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return Outcome::Crashed(format!("it couldn't be started: {e}")),
    };
    let pid = child.id();

    // stdout → events, one bounded line at a time.
    if let Some(stdout) = child.stdout.take() {
        let events = job.events.clone();
        let _ = thread::Builder::new()
            .name(format!("samo-ext-{}-out", job.id))
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    match read_line_capped(&mut reader, MAX_LINE) {
                        Ok(Some(line)) => {
                            if events.send(Event::Line(generation, line)).is_err() {
                                return;
                            }
                        }
                        Ok(None) | Err(_) => {
                            let _ = events.send(Event::Eof(generation));
                            return;
                        }
                    }
                }
            });
    }
    // stderr → helper.log.
    if let Some(stderr) = child.stderr.take() {
        let log = log.clone();
        let _ = thread::Builder::new()
            .name(format!("samo-ext-{}-err", job.id))
            .spawn(move || {
                let mut reader = BufReader::new(stderr);
                while let Ok(Some(line)) = read_line_capped(&mut reader, MAX_LINE) {
                    match line {
                        Line::Text(text) => log.note(&format!("stderr: {text}")),
                        Line::TooLong(n) => log.note(&format!("stderr: ({n} bytes, cut)")),
                    }
                }
            });
    }
    // writer ← messages for the helper. It owns stdin; dropping its sender closes stdin.
    let (to_helper, from_host) = mpsc::channel::<HostMsg>();
    if let Some(mut stdin) = child.stdin.take() {
        let _ = thread::Builder::new()
            .name(format!("samo-ext-{}-in", job.id))
            .spawn(move || {
                for msg in from_host {
                    let Ok(line) = encode(&msg) else { continue };
                    if writeln!(stdin, "{line}")
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        return;
                    }
                }
            });
    }
    let _ = to_helper.send(HostMsg::Hello {
        protocol: PROTOCOL_VERSION,
        samo_version: config.samo_version.clone(),
        data_dir: job.data_dir.to_string_lossy().into_owned(),
    });

    let started = Instant::now();
    let mut said_hello = false;
    let mut messages = RateGate::messages();
    let mut held = Held::default();
    let mut run = Run {
        job,
        log,
        popups: RateGate::popups(),
        unhealthy: None,
    };
    let mut sys = System::new();
    let mut cpu: Vec<f32> = Vec::new();
    let mut next_check = Instant::now() + config.resource_every;

    loop {
        let event = rx.recv_timeout(TICK);
        match event {
            Ok(Event::Line(g, line)) if g == generation => {
                let text = match line {
                    Line::TooLong(n) => {
                        log.note(&format!(
                            "dropped a {n}-byte line (the limit is {MAX_LINE})"
                        ));
                        continue;
                    }
                    Line::Text(text) if text.trim().is_empty() => continue,
                    Line::Text(text) => text,
                };
                let msg = match decode_helper_line(&text) {
                    Ok(msg) => msg,
                    Err(e) => {
                        log.note(&format!("dropped a line that isn't protocol v1: {e}"));
                        continue;
                    }
                };
                if !said_hello {
                    match msg {
                        HelperMsg::Hello { protocol } if protocol == PROTOCOL_VERSION => {
                            said_hello = true;
                            job.inner.update(job.slot, |rt| rt.status = Status::Running);
                        }
                        HelperMsg::Hello { protocol } => {
                            stop_child(&mut child, to_helper, config.grace);
                            return Outcome::Halt(format!(
                                "It speaks protocol {protocol}; this Samo speaks {PROTOCOL_VERSION}"
                            ));
                        }
                        _ => log.note("dropped a message sent before hello"),
                    }
                    continue;
                }
                if messages.admit(now_ms()) {
                    run.apply(msg);
                } else {
                    match msg {
                        HelperMsg::View { sections } => held.view = Some(sections),
                        HelperMsg::Badge { count, summary } => held.badge = Some((count, summary)),
                        HelperMsg::Health { ok, message } => held.health = Some((ok, message)),
                        HelperMsg::Popup { .. }
                        | HelperMsg::Log { .. }
                        | HelperMsg::Hello { .. } => {
                            log.note("dropped a message: more than 20 a second");
                        }
                    }
                }
            }
            // From an earlier run of the helper.
            Ok(Event::Line(..)) => {}
            Ok(Event::Eof(g)) if g == generation => {
                return ended(&mut child, run.unhealthy, said_hello, config.grace);
            }
            Ok(Event::Eof(_)) => {}
            Ok(Event::Send(msg)) => {
                let _ = to_helper.send(msg);
            }
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => {
                stop_child(&mut child, to_helper, config.grace);
                return Outcome::Stopped;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        // Every pass, however it was woken.
        if !said_hello && started.elapsed() > config.hello_timeout {
            stop_child(&mut child, to_helper, Duration::ZERO);
            return Outcome::Halt(format!(
                "It didn't say hello within {}s",
                config.hello_timeout.as_secs_f32()
            ));
        }
        if !held.is_empty() && messages.admit(now_ms()) {
            let taken = std::mem::take(&mut held);
            if let Some(sections) = taken.view {
                run.apply(HelperMsg::View { sections });
            }
            if let Some((count, summary)) = taken.badge {
                run.apply(HelperMsg::Badge { count, summary });
            }
            if let Some((ok, message)) = taken.health {
                run.apply(HelperMsg::Health { ok, message });
            }
        }
        if let Ok(Some(_)) = child.try_wait() {
            // It exited, but something else still holds its stdout open.
            return ended(&mut child, run.unhealthy, said_hello, config.grace);
        }
        if Instant::now() >= next_check {
            next_check = Instant::now() + config.resource_every;
            let target = Pid::from_u32(pid);
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[target]),
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );
            if let Some(process) = sys.process(target) {
                let rss = process.memory();
                if rss > config.rss_limit_bytes {
                    stop_child(&mut child, to_helper, Duration::ZERO);
                    return Outcome::Halt(format!(
                        "It used {} MB of memory (the limit is {} MB)",
                        rss / (1024 * 1024),
                        config.rss_limit_bytes / (1024 * 1024)
                    ));
                }
                cpu.push(process.cpu_usage());
                if cpu.len() > config.cpu_samples {
                    cpu.remove(0);
                }
                let samples = cpu.len().max(1) as f32;
                let average = cpu.iter().sum::<f32>() / samples;
                if cpu.len() >= config.cpu_samples && average > config.cpu_limit_pct {
                    stop_child(&mut child, to_helper, Duration::ZERO);
                    return Outcome::Halt(format!(
                        "It used {average:.0}% of a CPU core for a minute (the limit is {:.0}%)",
                        config.cpu_limit_pct
                    ));
                }
            }
        }
    }
}

/// The helper ended by itself.
fn ended(
    child: &mut Child,
    unhealthy: Option<String>,
    said_hello: bool,
    grace: Duration,
) -> Outcome {
    let status = wait_up_to(child, grace);
    if let Some(why) = unhealthy {
        return Outcome::Halt(why);
    }
    let how = status.map_or_else(
        || "it stopped answering".to_string(),
        |s| format!("it exited ({s})"),
    );
    if said_hello {
        Outcome::Crashed(how)
    } else {
        Outcome::Crashed(format!("{how} before saying hello"))
    }
}

fn wait_up_to(child: &mut Child, grace: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                // Samo still holds the `Child` and hasn't reaped it, so its pid can't have been reused (R4).
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// `shutdown`, close stdin, then kill after `grace`.
fn stop_child(child: &mut Child, to_helper: Sender<HostMsg>, grace: Duration) {
    let _ = to_helper.send(HostMsg::Shutdown);
    drop(to_helper);
    let _ = wait_up_to(child, grace);
}

/// Keeps a view within `MAX_ROWS`, saying so if something was left out.
fn cap_rows(view: Vec<Widget>) -> Vec<Widget> {
    if rows_in(&view) <= MAX_ROWS {
        return view;
    }
    let mut kept = Vec::new();
    let mut used = 0usize;
    for widget in view {
        let room = MAX_ROWS.saturating_sub(used + 1);
        match widget {
            Widget::Rows { mut items } if items.len() > room => {
                items.truncate(room);
                kept.push(Widget::Rows { items });
                break;
            }
            other => {
                used += rows_in(std::slice::from_ref(&other));
                if used >= MAX_ROWS {
                    break;
                }
                kept.push(other);
            }
        }
    }
    kept.push(Widget::Note {
        text: format!("Some rows were left out: a page shows at most {MAX_ROWS}."),
        kind: NoteKind::Plain,
    });
    kept
}

impl Run<'_> {
    fn apply(&mut self, msg: HelperMsg) {
        let job = self.job;
        match msg {
            HelperMsg::Hello { .. } => {}
            HelperMsg::View { sections } => {
                let view = Arc::new(cap_rows(sections));
                job.inner.update(job.slot, |rt| rt.view = view);
            }
            HelperMsg::Badge { count, summary } => {
                job.inner.update(job.slot, |rt| {
                    rt.badge = count;
                    rt.badge_summary = summary;
                });
            }
            HelperMsg::Popup {
                key,
                kind,
                title,
                body,
            } => {
                if self.popups.admit(now_ms()) {
                    job.inner.push_popup(PendingPopup {
                        slot: job.slot,
                        key,
                        kind,
                        title,
                        body,
                    });
                } else {
                    self.log
                        .note("dropped a pop-up: 3 s apart and 10 a minute at most");
                }
            }
            HelperMsg::Log { event, detail } => {
                let event: String = event.chars().take(40).collect();
                let detail: String = detail.chars().take(1_000).collect();
                History::append(&job.inner.root, &event, &job.id, &detail);
            }
            HelperMsg::Health { ok, message } => {
                self.unhealthy = (!ok).then(|| message.clone());
                job.inner.update(job.slot, |rt| {
                    rt.note = (!message.is_empty()).then_some(message);
                });
            }
        }
    }
}
