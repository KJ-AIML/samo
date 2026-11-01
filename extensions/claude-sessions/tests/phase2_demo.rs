//! ADR-002 Phase 2's demo, end to end and automated: Samo's real extension host runs the real `samo-claude`,
//! installed from a dev folder, and two stand-ins for Claude Code run the exact hook commands Samo put in
//! `settings.json`. Everything happens in a temporary home folder; the real `~/.claude` is never touched.
//!
//! The steps follow ADR-002 §6 Phase 2 "done when": connect (hooks and a backup), two sessions with a permission
//! wait, a badge and pop-ups, a terminal closed without SessionEnd, today's tokens, plan limits (and a status line
//! that isn't Samo's), disconnect and off/on restoring the settings, and hooks that stay async and silent.

use samo_ext_protocol::{Lamp, PopupKind, Widget};
use samo_extensions::{ExtRoot, ExtSlot, ExtSnapshot, Host, HostConfig, Status};
use samo_primitives::Reading;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const SLOT: ExtSlot = ExtSlot(0);

type Fallible<T> = Result<T, String>;

fn io<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// One stand-in Claude Code process, running hooks for one session.
struct Claude {
    child: Child,
    stdin: ChildStdin,
    reports: BufReader<ChildStdout>,
    session: String,
    cwd: String,
    transcript: String,
}

impl Claude {
    fn start(home: &Path, session: &str, project: &str, transcript: &Path) -> Fallible<Self> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fake-claude"))
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(io)?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        Ok(Self {
            child,
            stdin,
            reports: BufReader::new(stdout),
            session: session.into(),
            cwd: format!("/Users/kj/Developer/{project}"),
            transcript: transcript.to_string_lossy().into_owned(),
        })
    }

    /// Fires one hook event and checks that every hook ran, succeeded and printed nothing (R9).
    fn event(&mut self, name: &str, extra: Value) -> Fallible<()> {
        let mut payload = json!({
            "session_id": self.session,
            "transcript_path": self.transcript,
            "cwd": self.cwd,
            "permission_mode": "default",
            "hook_event_name": name,
        });
        if let (Some(p), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        writeln!(self.stdin, "{payload}").map_err(io)?;
        self.stdin.flush().map_err(io)?;
        let mut line = String::new();
        self.reports.read_line(&mut line).map_err(io)?;
        let report: Value = serde_json::from_str(&line).map_err(io)?;
        if report["commands"] != 1 || report["printed"] != 0 || report["ok"] != true {
            return Err(format!("hook for {name} misbehaved: {report}"));
        }
        Ok(())
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Claude {
    fn drop(&mut self) {
        self.kill();
    }
}

fn wait_for(
    host: &Host,
    secs: u64,
    what: &str,
    test: impl Fn(&ExtSnapshot) -> bool,
) -> Fallible<Arc<ExtSnapshot>> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let snap = host.snapshot();
        if test(&snap) {
            return Ok(snap);
        }
        if Instant::now() > deadline {
            let view = snap
                .get(SLOT)
                .map(|e| serde_json::to_string(&*e.view).unwrap_or_default());
            return Err(format!(
                "timed out waiting for {what}; status {:?}; view {view:?}",
                snap.get(SLOT).map(|e| &e.status)
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn view(snap: &ExtSnapshot) -> Vec<Widget> {
    snap.get(SLOT)
        .map(|e| (*e.view).clone())
        .unwrap_or_default()
}

/// The session rows: (project, lamp, detail).
fn rows(snap: &ExtSnapshot) -> Vec<(String, Lamp, String)> {
    view(snap)
        .into_iter()
        .filter_map(|w| match w {
            Widget::Rows { items } => Some(items),
            _ => None,
        })
        .flatten()
        .map(|i| (i.title, i.lamp, i.detail))
        .collect()
}

fn row<'a>(
    rows: &'a [(String, Lamp, String)],
    project: &str,
) -> Option<&'a (String, Lamp, String)> {
    rows.iter().find(|r| r.0 == project)
}

fn switch(snap: &ExtSnapshot, key: &str) -> Option<(bool, Option<String>)> {
    view(snap).into_iter().find_map(|w| match w {
        Widget::Switch {
            key: k, on, note, ..
        } if k == key => Some((on, note)),
        _ => None,
    })
}

fn figure(snap: &ExtSnapshot) -> Option<(Reading<f64>, Option<String>)> {
    view(snap).into_iter().find_map(|w| match w {
        Widget::Figure { value, note, .. } => Some((value, note)),
        _ => None,
    })
}

fn meter(snap: &ExtSnapshot, label: &str) -> Option<Reading<f64>> {
    view(snap).into_iter().find_map(|w| match w {
        Widget::Meter {
            label: l, fraction, ..
        } if l == label => Some(fraction),
        _ => None,
    })
}

fn read_json(path: &Path) -> Fallible<Value> {
    serde_json::from_slice(&fs::read(path).map_err(io)?).map_err(io)
}

/// A transcript line, at `ts`.
fn usage(msg: &str, session: &str, ts: &str, input: u64, output: u64, cache_read: u64) -> String {
    json!({
        "type": "assistant",
        "sessionId": session,
        "timestamp": ts,
        "requestId": format!("req_{msg}"),
        "message": {
            "id": msg,
            "usage": {
                "input_tokens": input,
                "output_tokens": output,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": cache_read
            }
        }
    })
    .to_string()
        + "\n"
}

fn rfc3339(t: chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The dev folder: the manifest and a copy of the helper (a copy, not cargo's hard link: R5).
fn dev_package(dir: &Path) -> Fallible<PathBuf> {
    let dev = dir.join("dev/claude-sessions");
    fs::create_dir_all(dev.join("bin")).map_err(io)?;
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/extension.toml"),
        dev.join("extension.toml"),
    )
    .map_err(io)?;
    let helper = dev.join("bin/samo-claude");
    fs::write(
        &helper,
        fs::read(env!("CARGO_BIN_EXE_samo-claude")).map_err(io)?,
    )
    .map_err(io)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).map_err(io)?;
    Ok(dev)
}

#[test]
fn phase2_end_to_end_demo() -> Fallible<()> {
    let dir = tempfile::tempdir().map_err(io)?;
    let home = dir.path().join("home");
    let claude_dir = home.join(".claude");
    let projects = claude_dir.join("projects/-Users-kj-Developer-samo");
    fs::create_dir_all(projects.join("session-a/subagents")).map_err(io)?;
    let settings_path = claude_dir.join("settings.json");
    let original = json!({ "model": "opus", "permissions": { "allow": ["Bash(ls:*)"] } });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&original).map_err(io)?,
    )
    .map_err(io)?;

    // Today's transcripts, before Samo starts: a message written three times, another session's copy of it, a
    // line from before midnight, and a subagent's.
    let now = chrono::Utc::now();
    let today = rfc3339(now - chrono::Duration::seconds(1));
    let midnight = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|m| m.and_local_timezone(chrono::Local).earliest())
        .map(|m| m.with_timezone(&chrono::Utc))
        .ok_or("no local midnight")?;
    let yesterday = rfc3339(midnight - chrono::Duration::hours(1));
    let transcript_a = projects.join("session-a.jsonl");
    let transcript_b = projects.join("session-b.jsonl");
    let a_lines = usage("m1", "session-a", &today, 10, 100, 1_000).repeat(3)
        + &usage("old", "session-a", &yesterday, 5_000, 5_000, 5_000)
        + &usage("m2", "session-a", &today, 1, 2, 3);
    fs::write(&transcript_a, a_lines).map_err(io)?;
    fs::write(
        &transcript_b,
        usage("m1", "session-b", &today, 10, 100, 1_000)
            + &usage("m3", "session-b", &today, 7, 0, 0),
    )
    .map_err(io)?;
    fs::write(
        projects.join("session-a/subagents/agent-1.jsonl"),
        usage("m4", "session-a", &today, 20, 30, 0),
    )
    .map_err(io)?;
    // m1 (1,110) + m2 (6) + m3 (7) + m4 (50); nothing from before midnight, nothing twice.
    let expected_tokens = 1_110.0 + 6.0 + 7.0 + 50.0;

    // Samo, with Claude Sessions installed from the dev folder.
    let dev = dev_package(dir.path())?;
    let root = ExtRoot::at(home.join("Library/Application Support/Samo/Extensions"));
    let mut config = HostConfig::new("0.1.0");
    config.dev_dir = Some(dev.to_string_lossy().into_owned());
    config.home = Some(home.clone());
    config.wake_every = Duration::from_millis(10);
    let host = Host::start(root.clone(), config, Arc::new(|| {}));
    wait_for(&host, 10, "Claude Sessions to run", |s| {
        s.get(SLOT)
            .is_some_and(|e| e.status == Status::Running && !e.view.is_empty())
    })?;

    // 1. Connect: Samo's hook under all ten events, async and guarded, and a backup of the original.
    host.setting(SLOT, "connect", true);
    let snap = wait_for(&host, 10, "Connect to show on", |s| {
        switch(s, "connect").is_some_and(|(on, _)| on)
    })?;
    let connected = read_json(&settings_path)?;
    for event in samo_claude::settings::HOOK_EVENTS {
        let entries = connected["hooks"][event]
            .as_array()
            .ok_or(format!("no {event}"))?;
        assert_eq!(entries.len(), 1, "{event}");
        assert_eq!(
            entries[0]["hooks"][0],
            samo_claude::settings::hook_entry(),
            "{event}"
        );
        assert_eq!(entries[0]["hooks"][0]["async"], true);
    }
    assert_eq!(
        read_json(&claude_dir.join("settings.json.samo-bak"))?,
        original
    );
    assert_eq!(connected["model"], "opus");
    let history = fs::read_to_string(root.history_file()).map_err(io)?;
    assert!(history.contains("\"event\":\"connect\""), "{history}");

    // 4. Today's tokens, exactly, once the transcripts are read.
    let snap = wait_for(&host, 15, "today's tokens", |s| {
        figure(s).is_some_and(|(v, note)| {
            v.as_value() == Some(&expected_tokens)
                && note.is_some_and(|n| !n.starts_with("Counting"))
        })
    })
    .map_err(|e| format!("{e}; figure was {:?}", figure(&snap)))?;
    let _ = snap;

    // 2. Two sessions. A works; B asks for permission: within a second, the row says so, the badge counts it,
    // and a pop-up is waiting.
    let mut a = Claude::start(&home, "session-a", "samo", &transcript_a)?;
    let mut b = Claude::start(&home, "session-b", "talvyn-mesh", &transcript_b)?;
    a.event("SessionStart", json!({"source": "startup"}))?;
    b.event("SessionStart", json!({"source": "startup"}))?;
    a.event("UserPromptSubmit", json!({"prompt": "fix it"}))?;
    b.event("UserPromptSubmit", json!({"prompt": "add it"}))?;
    wait_for(&host, 5, "both working", |s| {
        let r = rows(s);
        row(&r, "samo").is_some_and(|x| x.1 == Lamp::Working)
            && row(&r, "talvyn-mesh").is_some_and(|x| x.1 == Lamp::Working)
    })?;
    let _ = host.take_popups();

    let asked = Instant::now();
    b.event(
        "PermissionRequest",
        json!({"tool_name": "Bash", "tool_input": {"command": "rm -r build"}}),
    )?;
    let snap = wait_for(&host, 5, "B to wait for permission", |s| {
        let r = rows(s);
        row(&r, "talvyn-mesh")
            .is_some_and(|x| x.1 == Lamp::NeedsYou && x.2.starts_with("permission: Bash"))
    })?;
    let took = asked.elapsed();
    assert!(
        took < Duration::from_secs(1),
        "the wait showed after {took:?}"
    );
    let ext = snap.get(SLOT).ok_or("no extension")?;
    assert_eq!(ext.badge, 1);
    assert_eq!(ext.badge_summary, "Claude · 1 needs you · 1 working");
    assert_eq!(snap.attention(), 1);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut popups = Vec::new();
    while popups.is_empty() && Instant::now() < deadline {
        popups = host.take_popups();
        thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(popups.len(), 1, "{popups:?}");
    assert_eq!(popups[0].kind, PopupKind::NeedsYou);
    assert_eq!(popups[0].title, "talvyn-mesh needs you");

    // Approving it shows as the next tool event.
    b.event(
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_input": {}, "tool_response": {}}),
    )?;
    wait_for(&host, 5, "B back to work", |s| {
        row(&rows(s), "talvyn-mesh").is_some_and(|x| x.1 == Lamp::Working) && s.attention() == 0
    })?;

    // A turn of 15 s or more that ends pops up "finished". (Pop-ups are 3 s apart at most, so wait that out.)
    let file_a = root
        .data_dir("claude-sessions")
        .join("sessions/session-a.json");
    let mut session_a = read_json(&file_a)?;
    let twenty_ago = chrono::Utc::now().timestamp_millis() - 20_000;
    session_a["turn_started_ms"] = json!(twenty_ago);
    // Replaced by a rename, as the hook does, so serve sees the folder change.
    let tmp = file_a.with_extension("tmp-test");
    fs::write(&tmp, session_a.to_string()).map_err(io)?;
    fs::rename(&tmp, &file_a).map_err(io)?;
    thread::sleep(Duration::from_millis(3_100));
    let _ = host.take_popups();
    a.event(
        "Stop",
        json!({"stop_hook_active": false, "background_tasks": []}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut finished = Vec::new();
    while finished.is_empty() && Instant::now() < deadline {
        finished = host.take_popups();
        thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(
        finished.first().map(|p| p.kind),
        Some(PopupKind::Finished),
        "{finished:?}"
    );
    assert_eq!(
        finished.first().map(|p| p.title.as_str()),
        Some("samo finished")
    );

    // 3. B's terminal closes without SessionEnd: its row goes within 10 s.
    b.kill();
    let gone = Instant::now();
    wait_for(&host, 10, "B's row to go", |s| {
        row(&rows(s), "talvyn-mesh").is_none()
    })?;
    assert!(gone.elapsed() < Duration::from_secs(10));
    assert!(
        row(&rows(&host.snapshot()), "samo").is_some(),
        "A is still there"
    );

    // 5. Plan limits: Samo's status line, fed by Claude Code, fills both meters.
    host.setting(SLOT, "plan_limits", true);
    wait_for(&host, 10, "Plan limits on", |s| {
        switch(s, "plan_limits").is_some_and(|(on, _)| on)
    })?;
    let with_line = read_json(&settings_path)?;
    let line_command = with_line["statusLine"]["command"]
        .as_str()
        .ok_or("no status line")?
        .to_string();
    let soon = chrono::Utc::now().timestamp() + 7_200;
    let status_input = json!({
        "session_id": "session-a",
        "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
        "rate_limits": {
            "five_hour": {"used_percentage": 34, "resets_at": soon},
            "seven_day": {"used_percentage": 12, "resets_at": soon + 86_400}
        }
    });
    let mut line = Command::new("/bin/sh")
        .arg("-c")
        .arg(&line_command)
        .env_clear()
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(io)?;
    if let Some(mut stdin) = line.stdin.take() {
        stdin
            .write_all(status_input.to_string().as_bytes())
            .map_err(io)?;
    }
    let printed = line.wait_with_output().map_err(io)?;
    assert!(printed.status.success());
    assert_eq!(
        String::from_utf8_lossy(&printed.stdout),
        "5h 34% · wk 12%\n"
    );
    wait_for(&host, 5, "both meters", |s| {
        meter(s, "5-hour")
            .and_then(|m| m.as_value().copied())
            .is_some_and(|v| (v - 0.34).abs() < 1e-9)
            && meter(s, "Weekly")
                .and_then(|m| m.as_value().copied())
                .is_some_and(|v| (v - 0.12).abs() < 1e-9)
    })?;

    // A status line that isn't Samo's: Plan limits refuses, and says why.
    host.setting(SLOT, "plan_limits", false);
    wait_for(&host, 10, "Plan limits off", |s| {
        switch(s, "plan_limits").is_some_and(|(on, _)| !on)
    })?;
    let mut theirs = read_json(&settings_path)?;
    theirs["statusLine"] = json!({"type": "command", "command": "~/bin/my-line.sh"});
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&theirs).map_err(io)?,
    )
    .map_err(io)?;
    host.setting(SLOT, "plan_limits", true);
    let snap = wait_for(&host, 10, "the refusal", |s| {
        switch(s, "plan_limits")
            .is_some_and(|(on, note)| !on && note.is_some_and(|n| n.contains("never replaces")))
    })?;
    let _ = snap;
    assert_eq!(
        read_json(&settings_path)?["statusLine"]["command"],
        "~/bin/my-line.sh"
    );
    let mut without = read_json(&settings_path)?;
    if let Some(o) = without.as_object_mut() {
        o.remove("statusLine");
    }
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&without).map_err(io)?,
    )
    .map_err(io)?;

    // 6. Disconnect: the settings are as they were. Off and on take the hooks out and put them back.
    host.setting(SLOT, "connect", false);
    wait_for(&host, 10, "Connect off", |s| {
        switch(s, "connect").is_some_and(|(on, _)| !on)
    })?;
    assert_eq!(read_json(&settings_path)?, original);

    host.setting(SLOT, "connect", true);
    wait_for(&host, 10, "Connect on again", |s| {
        switch(s, "connect").is_some_and(|(on, _)| on)
    })?;
    host.set_on(SLOT, false);
    wait_for(&host, 10, "the extension off", |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Off)
    })?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while read_json(&settings_path)? != original {
        if Instant::now() > deadline {
            return Err(format!("off left {}", read_json(&settings_path)?));
        }
        thread::sleep(Duration::from_millis(25));
    }
    host.set_on(SLOT, true);
    wait_for(&host, 10, "the extension on", |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Running)
            && switch(s, "connect").is_some_and(|(on, _)| on)
    })?;
    assert!(samo_claude::settings::is_connected(&read_json(
        &settings_path
    )?));

    // 7. The hooks stayed silent and succeeded throughout (each `event` call checked it); one more to be sure.
    a.event("PreToolUse", json!({"tool_name": "Read", "tool_input": {}}))?;

    host.shutdown_all();
    Ok(())
}
