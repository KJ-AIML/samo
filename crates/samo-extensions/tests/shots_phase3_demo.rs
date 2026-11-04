//! These run real helpers: `#!/bin/sh` stand-ins, Swift or codesign. Their Windows forms come with #9 (ADR-006).
#![cfg(unix)]

//! ADR-004 Phase 3's demo, the parts a machine can check: the real `samo-shots` 0.3.0, installed from a store signed
//! with a test key, under Samo's real host. Reading itself is covered by Shots' own tests on rendered Thai, English
//! and QR images (`RecognizeTests`); reading what is on screen needs Screen Recording, so a person checks it
//! (M14–M18).
//!
//! The only shortcut set is ⌃⌥⇧⌘F19 for copying text, so the test takes no key anyone uses.

mod swift_common;

use samo_ext_protocol::{RowItem, Widget};
use samo_extensions::{ExtRoot, ExtSnapshot, Host, HostConfig, Status};
use samo_testkit::store::{FakeStore, listing_for};
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use swift_common::{
    Fallible, SLOT, build_shots, chosen, has_section, io, page, shots_processes, stage,
    version_at_least, wait,
};

fn buttons(snap: &ExtSnapshot) -> Vec<String> {
    page(snap)
        .into_iter()
        .filter_map(|w| match w {
            Widget::Button { action, .. } => Some(action),
            _ => None,
        })
        .collect()
}

/// The Shortcuts section's rows.
fn shortcut_rows(snap: &ExtSnapshot) -> Vec<RowItem> {
    page(snap)
        .into_iter()
        .filter_map(|w| match w {
            Widget::Rows { items } if items.iter().any(|r| r.title == "Whole screen") => {
                Some(items)
            }
            _ => None,
        })
        .next()
        .unwrap_or_default()
}

#[test]
fn shots_phase3_demo() -> Fallible<()> {
    let binary = build_shots()?;
    let dir = tempfile::tempdir().map_err(io)?;
    let home = dir.path().join("home");
    let root = ExtRoot::at(home.join("Library/Application Support/Samo/Extensions"));
    fs::create_dir_all(root.data_dir("shots")).map_err(io)?;
    // ⌃⌥⇧⌘F19 (key code 80) for copying text; nothing else.
    fs::write(
        root.data_dir("shots").join("settings.json"),
        r#"{"after":"both","then":"edit","folder":"~/Pictures/Shots","format":"png","retina":true,"sound":false,
            "shortcuts":{"text":{"keyCode":80,"modifiers":6912}}}"#,
    )
    .map_err(io)?;

    let shop = FakeStore::new()?;
    let manifest = stage(&dir.path().join("stage"), &binary, |t| t)?;
    assert!(
        version_at_least(&manifest.version, (0, 3, 0)),
        "reading came in 0.3.0"
    );
    assert!(
        manifest
            .windows
            .as_deref()
            .is_some_and(|w| w.contains("a card for what it reads")),
        "the store says it opens a card: {:?}",
        manifest.windows
    );
    let package = shop.put_package(&dir.path().join("stage"), "shots", &manifest.version)?;
    let bytes = fs::read(shop.dir.path().join(&package.file)).map_err(io)?;
    let listing = listing_for(&manifest, package);

    let mut config = HostConfig::new("0.3.0");
    config.home = Some(home.clone());
    config.wake_every = Duration::from_millis(10);
    let host = Host::start(root.clone(), config, Arc::new(|| {}));
    host.install_from_store(&listing, &bytes).map_err(io)?;
    let snap = wait(&host, "its page", 20, |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Running) && has_section(s, "SHORTCUTS")
    })?;

    // 1. Phase 2's choices carry over: every shot still opens in the editor.
    assert_eq!(chosen(&snap, "then").as_deref(), Some("edit"));

    // 2. Copy text, pick a colour and measure, from Samo's page.
    let actions = buttons(&snap);
    for action in ["area", "window", "screen", "text", "colour", "ruler"] {
        assert!(
            actions.iter().any(|a| a == action),
            "no {action} button in {actions:?}"
        );
    }

    // 3. Copying text, picking a colour and measuring have shortcut rows (found by name: later versions add more).
    //    Copying text has the one it was given, registered and ready; the others have none.
    let rows = shortcut_rows(&snap);
    let row = |title: &str| rows.iter().find(|r| r.title == title).cloned();
    for title in [
        "Whole screen",
        "An area",
        "Copy text",
        "Pick a colour",
        "Measure",
    ] {
        assert!(row(title).is_some(), "no {title} row");
    }
    let text = row("Copy text").ok_or("no Copy text row")?;
    assert_eq!(
        text.detail, "Ready",
        "the shortcut registered: {:?}",
        text.trailing
    );
    assert_eq!(text.trailing, "⌃⌥⇧⌘key 80");
    for title in ["Pick a colour", "Measure"] {
        assert_eq!(
            row(title).map(|r| r.detail),
            Some("No shortcut".to_string()),
            "{title}"
        );
    }
    // Shottr's two keys aren't in these settings, so they aren't taken either.
    assert_eq!(
        row("Whole screen").map(|r| r.detail),
        Some("No shortcut".to_string())
    );

    // 4. It stays up (R15, its limits), then Off leaves nothing running.
    thread::sleep(Duration::from_secs(3));
    assert_eq!(
        host.snapshot().get(SLOT).map(|e| e.status.clone()),
        Some(Status::Running)
    );
    host.set_on(SLOT, false);
    let deadline = Instant::now() + Duration::from_secs(10);
    while shots_processes(&root) > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        shots_processes(&root),
        0,
        "turning Shots off leaves nothing running"
    );
    host.shutdown_all();
    Ok(())
}
