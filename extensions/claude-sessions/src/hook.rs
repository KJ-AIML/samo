//! `samo-claude hook`: what Claude Code runs on each session event (ADR-002 C1, R9).
//!
//! It must never get in Claude Code's way: it prints nothing (Claude Code reads some hooks' output as context),
//! always exits 0 (2 would block), gives up within 500 ms whatever happens, reads at most 1 MiB, and does one
//! small file write. No transcript reading, no network.

use crate::ancestry;
use crate::paths;
use crate::sessions::{Apply, HookEvent, SessionFile, apply, valid_session_id};
use std::fs::{self, File, TryLockError};
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The most hook input read.
pub const MAX_INPUT: u64 = 1024 * 1024;
/// How long to wait for Claude Code's input. Claude Code writes it as it starts the hook, so this only matters
/// when something holds stdin open; it leaves room under the 500 ms promise for starting the process on a busy
/// machine.
const READ_WAIT: Duration = Duration::from_millis(200);
/// Everything is done by then, or given up.
const DEADLINE: Duration = Duration::from_millis(450);

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Runs the hook. Every failure is silent: the worst case is that Samo misses one event.
pub fn run() {
    let started = Instant::now();
    // The event's time is when the hook started, so events keep their order however long each hook takes.
    let at_ms = now_ms();
    let (tx, rx) = mpsc::channel();
    let _ = thread::Builder::new().spawn(move || {
        let mut input = Vec::new();
        let _ = std::io::stdin().take(MAX_INPUT + 1).read_to_end(&mut input);
        let _ = tx.send(input);
    });
    let Ok(input) = rx.recv_timeout(READ_WAIT) else {
        return;
    };
    if u64::try_from(input.len()).unwrap_or(u64::MAX) > MAX_INPUT {
        return;
    }
    let Ok(event) = serde_json::from_slice::<HookEvent>(&input) else {
        return;
    };
    let Some(dir) = paths::sessions_dir() else {
        return;
    };
    record(&dir, &event, at_ms, started);
}

/// Applies `event` to its session's file, under a per-session lock so two hooks never overwrite each other.
pub fn record(dir: &Path, event: &HookEvent, at_ms: i64, started: Instant) {
    if !valid_session_id(&event.session_id) || fs::create_dir_all(dir).is_err() {
        return;
    }
    let file = dir.join(format!("{}.json", event.session_id));
    let Ok(lock) = File::create(dir.join(format!("{}.lock", event.session_id))) else {
        return;
    };
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if started.elapsed() < DEADLINE => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return,
        }
    }
    let prev: Option<SessionFile> = fs::read(&file)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    match apply(prev.as_ref(), event, at_ms) {
        Apply::Keep => {}
        Apply::Remove => {
            let _ = fs::remove_file(&file);
            let _ = fs::remove_file(dir.join(format!("{}.lock", event.session_id)));
        }
        Apply::Write(mut next) => {
            if next.claude.is_none() && started.elapsed() < Duration::from_millis(250) {
                next.claude = ancestry::claude_identity(&ancestry::ancestors(6));
            }
            let Ok(text) = serde_json::to_vec(&next) else {
                return;
            };
            let tmp = dir.join(format!(".{}.{}.tmp", event.session_id, std::process::id()));
            if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, &file).is_err() {
                let _ = fs::remove_file(&tmp);
            }
        }
    }
}
