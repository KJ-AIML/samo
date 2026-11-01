//! `samo-claude statusline`: what Claude Code runs for its status line while Plan limits is on (ADR-002 C3). It
//! saves the limits for Samo, prints one short line, and always exits 0.

use crate::hook::now_ms;
use crate::limits::{from_status_json, status_text};
use crate::paths;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const MAX_INPUT: u64 = 1024 * 1024;

pub fn run() {
    let (tx, rx) = mpsc::channel();
    let _ = thread::Builder::new().spawn(move || {
        let mut input = Vec::new();
        let _ = std::io::stdin().take(MAX_INPUT + 1).read_to_end(&mut input);
        let _ = tx.send(input);
    });
    let Ok(input) = rx.recv_timeout(Duration::from_millis(500)) else {
        return;
    };
    let Ok(v) = serde_json::from_slice::<Value>(&input) else {
        return;
    };
    let limits = from_status_json(&v, now_ms());
    if limits.any()
        && let Some(dir) = paths::data_dir()
        && fs::create_dir_all(&dir).is_ok()
        && let Ok(text) = serde_json::to_vec(&limits)
    {
        let tmp = dir.join(format!(".limits.{}.tmp", std::process::id()));
        if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, dir.join("limits.json")).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", status_text(&limits));
}
