//! A stand-in for Claude Code in tests. Each line on its stdin is a hook payload: it runs every command
//! `~/.claude/settings.json` lists for that event, through `/bin/sh -c` as Claude Code does, with the payload on
//! the command's stdin, and reports one line: how many commands ran, how many bytes they printed, and whether
//! they all succeeded. It keeps running after its input ends, as Claude Code does, until it is killed, so a test
//! can "close the terminal".

use serde_json::Value;
use std::io::{BufRead, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn commands_for(event: &str) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let settings = std::fs::read(format!("{home}/.claude/settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null);
    settings["hooks"][event]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .filter_map(|g| g["hooks"].as_array())
                .flatten()
                .filter_map(|h| h["command"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn main() {
    let mut out = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(payload) = line else { break };
        let event = serde_json::from_str::<Value>(&payload)
            .ok()
            .and_then(|v| v["hook_event_name"].as_str().map(String::from))
            .unwrap_or_default();
        let commands = commands_for(&event);
        let mut printed = 0usize;
        let mut ok = true;
        for command in &commands {
            let Ok(mut child) = Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                ok = false;
                continue;
            };
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(payload.as_bytes());
            }
            match child.wait_with_output() {
                Ok(output) => {
                    printed += output.stdout.len();
                    ok &= output.status.success();
                }
                Err(_) => ok = false,
            }
        }
        let report = serde_json::json!({
            "event": event,
            "commands": commands.len(),
            "printed": printed,
            "ok": ok,
        });
        let _ = writeln!(out, "{report}");
        let _ = out.flush();
    }
    loop {
        std::thread::sleep(Duration::from_secs(3_600));
    }
}
