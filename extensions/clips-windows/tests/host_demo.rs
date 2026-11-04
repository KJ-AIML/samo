//! PR E's demo, end to end: the real `samo-clips.exe` under Samo's real extension host, on this PC's real clipboard.
//! A copy shows on Clips' page in Samo; a copy marked secret doesn't; Delete and Pause from Samo work; turning Clips
//! off stops it and keeps the history. The shortcut is left unset, so the test never takes one from this PC.
//!
//! It replaces what you copied, so it runs on CI, or when `SAMO_TEST_CLIPBOARD=1`; the text that was there goes back.
#![cfg(windows)]

use samo_clips::watcher::{Board, Put, clip};
use samo_clips::win::board::WinBoard;
use samo_ext_protocol::{Lamp, RowItem, Widget};
use samo_extensions::{ExtRoot, ExtSlot, ExtSnapshot, Host, HostConfig, Status};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type Fallible<T> = Result<T, String>;

const SLOT: ExtSlot = ExtSlot(0);

fn io(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn wait(
    host: &Host,
    what: &str,
    secs: u64,
    test: impl Fn(&ExtSnapshot) -> bool,
) -> Fallible<Arc<ExtSnapshot>> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let snap = host.snapshot();
        if test(&snap) {
            return Ok(snap);
        }
        if Instant::now() > deadline {
            return Err(format!(
                "timed out waiting for {what}; {:?}",
                snap.get(SLOT).map(|e| (&e.status, &e.note))
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn page(snap: &ExtSnapshot) -> Vec<Widget> {
    snap.get(SLOT)
        .map(|e| (*e.view).clone())
        .unwrap_or_default()
}

/// The rows right after the section titled `title`.
fn rows(snap: &ExtSnapshot, title: &str) -> Vec<RowItem> {
    let widgets = page(snap);
    let Some(at) = widgets
        .iter()
        .position(|w| matches!(w, Widget::Section { title: t, .. } if t == title))
    else {
        return Vec::new();
    };
    match widgets.get(at + 1) {
        Some(Widget::Rows { items }) => items.clone(),
        _ => Vec::new(),
    }
}

fn listed(snap: &ExtSnapshot, text: &str) -> Option<RowItem> {
    rows(snap, "RECENT").into_iter().find(|r| r.title == text)
}

fn copy(text: &str, secret: bool) {
    WinBoard::default().put(&Put {
        text: Some(text.into()),
        data: secret
            .then(|| (clip::EXCLUDE_FROM_MONITORING.to_string(), vec![0]))
            .into_iter()
            .collect(),
        files: Vec::new(),
    });
}

/// Clips as `ext-dev` lays it out: the real manifest and a copy of the helper (a copy, not cargo's hard link: R5).
fn lay_out(dev: &Path) -> Fallible<()> {
    let ext = dev.join("clips");
    fs::create_dir_all(ext.join("bin")).map_err(io)?;
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/extension.toml"),
        ext.join("extension.toml"),
    )
    .map_err(io)?;
    fs::copy(
        env!("CARGO_BIN_EXE_samo-clips"),
        ext.join("bin").join("samo-clips.exe"),
    )
    .map_err(io)?;
    Ok(())
}

/// Puts the text that was on the clipboard back.
struct GiveBack(Option<String>);

impl Drop for GiveBack {
    fn drop(&mut self) {
        if let Some(text) = self.0.take() {
            copy(&text, false);
        }
    }
}

#[test]
fn clips_on_windows_under_samos_host() -> Fallible<()> {
    if std::env::var_os("CI").is_none() && std::env::var_os("SAMO_TEST_CLIPBOARD").is_none() {
        eprintln!("skipped: the real clipboard demo runs on CI, or with SAMO_TEST_CLIPBOARD=1");
        return Ok(());
    }
    let _give_back = GiveBack(WinBoard::default().text());
    let dir = tempfile::tempdir().map_err(io)?;
    let dev = dir.path().join("dev");
    lay_out(&dev)?;
    let root = ExtRoot::at(dir.path().join("Extensions"));
    let data = root.data_dir("clips");
    fs::create_dir_all(&data).map_err(io)?;
    fs::write(
        data.join("settings.json"),
        r#"{"shortcut":null,"watching":true}"#,
    )
    .map_err(io)?;

    let mut config = HostConfig::new("0.4.1");
    config.dev_dir = Some(dev.to_string_lossy().into_owned());
    config.wake_every = Duration::from_millis(10);
    let host = Host::start(root.clone(), config, Arc::new(|| {}));
    host.set_on(SLOT, true);

    // 1. Its page in Samo: nothing yet, and no shortcut taken.
    let snap = wait(&host, "its page", 20, |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Running)
            && page(s)
                .iter()
                .any(|w| matches!(w, Widget::Section { title, .. } if title == "RECENT"))
    })?;
    assert_eq!(
        rows(&snap, "SHORTCUT").first().map(|r| r.detail.as_str()),
        Some("No shortcut: the list can't open")
    );
    assert!(
        !page(&snap)
            .iter()
            .any(|w| matches!(w, Widget::Section { title, .. } if title == "PERMISSIONS")),
        "Windows asks for no permission"
    );

    // 1b. A shortcut chosen on the page goes to the list, which tells whether Windows gave it (R18); none takes nothing.
    host.choice(SLOT, "shortcut", "alt-shift-v");
    let lamp = |s: &ExtSnapshot| {
        rows(s, "SHORTCUT")
            .first()
            .map(|r| (r.lamp, r.detail.clone()))
    };
    let snap = wait(&host, "the list to hold Alt+Shift+V", 10, |s| {
        lamp(s).is_some_and(|(l, _)| l == Lamp::Done || l == Lamp::Error)
    })?;
    eprintln!("Alt+Shift+V: {:?}", lamp(&snap));
    host.choice(SLOT, "shortcut", "none");
    wait(&host, "no shortcut", 10, |s| {
        lamp(s).is_some_and(|(l, d)| l == Lamp::Off && d.starts_with("No shortcut"))
    })?;

    // 2. A copy in another app shows on the page, with the badge.
    let text = format!("clips demo {}", std::process::id());
    copy(&text, false);
    let snap = wait(&host, "the copy on the page", 10, |s| {
        listed(s, &text).is_some()
    })?;
    let row = listed(&snap, &text).ok_or("not listed")?;
    assert_eq!(
        row.actions
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        ["copy", "pin", "delete"]
    );
    assert_eq!(
        snap.get(SLOT).map(|e| e.badge_summary.clone()),
        Some("Clips · 1 item".to_string())
    );

    // 3. A copy a password manager marks secret is never kept.
    let secret = format!("clips demo secret {}", std::process::id());
    copy(&secret, true);
    thread::sleep(Duration::from_millis(1500));
    assert!(
        listed(&host.snapshot(), &secret).is_none(),
        "a secret was kept"
    );
    let saved = fs::read_to_string(data.join("history.json")).map_err(io)?;
    assert!(!saved.contains(&secret) && saved.contains(&text));

    // 4. Pause from Samo keeps nothing until Resume.
    host.action(SLOT, "pause");
    wait(&host, "Resume on the page", 10, |s| {
        page(s)
            .iter()
            .any(|w| matches!(w, Widget::Button { action, .. } if action == "resume"))
    })?;
    let paused = format!("clips demo paused {}", std::process::id());
    copy(&paused, false);
    thread::sleep(Duration::from_millis(1500));
    assert!(
        listed(&host.snapshot(), &paused).is_none(),
        "kept while paused"
    );
    host.action(SLOT, "resume");

    // 5. Delete from Samo: gone from the page and from the history on disk.
    host.row_action(SLOT, &row.id.clone().unwrap_or_default(), "delete");
    wait(&host, "the deleted row to go", 10, |s| {
        listed(s, &text).is_none()
    })?;
    let saved = fs::read_to_string(data.join("history.json")).map_err(io)?;
    assert!(!saved.contains(&text));

    // 6. Off stops it; the history stays.
    host.set_on(SLOT, false);
    wait(&host, "Clips to stop", 10, |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Off)
    })?;
    assert!(data.join("history.json").is_file());
    host.shutdown_all();
    Ok(())
}
