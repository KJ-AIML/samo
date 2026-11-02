//! ADR-005 Phase 1's demo, the parts a machine can check: the real `samo-clips`, installed from a store signed with a
//! test key, under Samo's real host. Watching the clipboard needs Paste from Other Apps and pasting needs
//! Accessibility, both granted by a person, so those are KJ's checks by hand (M1–M6). Clips' own tests drive the
//! watcher, the history and the list.
//!
//! Clips' settings are written first with no shortcut and "Keep what I copy" off, so the test never takes ⇧⌘C from
//! the Mac it runs on, nor looks at its clipboard.

mod swift_common;

use samo_ext_protocol::{RowItem, Widget};
use samo_extensions::{ExtRoot, ExtSnapshot, Host, HostConfig, Status};
use samo_testkit::store::{FakeStore, listing_for};
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use swift_common::{
    Fallible, SLOT, build_helper, has_section, io, page, processes, stage_ext, version_at_least,
    wait,
};

const KEPT: &str = "9F2B5C66-0D3A-4C8E-9C4B-1A2B3C4D5E6F";
const PINNED: &str = "0B1C2D3E-4F50-4617-8293-A4B5C6D7E8F9";

/// A history as Clips writes it: one item kept, one pinned.
fn history_json() -> String {
    let item = |id: &str, text: &str, pinned: bool| {
        format!(
            r#"{{"id":"{id}","kind":"text","text":"{text}","files":[],"bytes":{},"first":"2026-11-02T10:00:00Z",
               "last":"2026-11-02T10:00:00Z","copies":1,"pinned":{pinned},"source":"com.apple.TextEdit",
               "fingerprint":"{id}"}}"#,
            text.len()
        )
    };
    format!(
        r#"{{"limit":200,"items":[{},{}]}}"#,
        item(PINNED, "a pinned snippet", true),
        item(KEPT, "hello from the demo", false)
    )
}

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

#[test]
fn clips_phase1_demo() -> Fallible<()> {
    let binary = build_helper("clips", "samo-clips")?;
    let dir = tempfile::tempdir().map_err(io)?;
    let home = dir.path().join("home");
    let root = ExtRoot::at(home.join("Library/Application Support/Samo/Extensions"));
    let data = root.data_dir("clips");
    fs::create_dir_all(&data).map_err(io)?;
    fs::write(
        data.join("settings.json"),
        r#"{"shortcut":null,"watching":false}"#,
    )
    .map_err(io)?;
    fs::write(data.join("history.json"), history_json()).map_err(io)?;

    let shop = FakeStore::new()?;
    let manifest = stage_ext(
        &dir.path().join("stage"),
        "clips",
        "samo-clips",
        &binary,
        |t| t,
    )?;
    assert!(version_at_least(&manifest.version, (0, 1, 0)));
    // 1. What it may do travels with the listing, before install (R20, R21).
    assert_eq!(
        manifest.permissions,
        ["paste-from-other-apps", "accessibility"]
    );
    assert!(
        manifest
            .watches
            .as_deref()
            .is_some_and(|w| w.contains("twice a second"))
    );
    assert!(manifest.windows.is_some());
    let package = shop.put_package(&dir.path().join("stage"), "clips", &manifest.version)?;
    let bytes = fs::read(shop.dir.path().join(&package.file)).map_err(io)?;
    let listing = listing_for(&manifest, package);
    assert_eq!(listing.watches, manifest.watches);

    // Clips needs Samo 0.4.0.
    let mut config = HostConfig::new("0.4.0");
    config.home = Some(home.clone());
    config.wake_every = Duration::from_millis(10);
    let host = Host::start(root.clone(), config, Arc::new(|| {}));
    host.install_from_store(&listing, &bytes).map_err(io)?;

    // 2. Its page: both permissions, the pinned and the recent, and no shortcut taken.
    let snap = wait(&host, "its page", 20, |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Running) && has_section(s, "RECENT")
    })?;
    let perms: Vec<String> = rows(&snap, "PERMISSIONS")
        .into_iter()
        .map(|r| r.title)
        .collect();
    assert_eq!(perms, ["Paste from Other Apps", "Accessibility"]);
    let pinned = rows(&snap, "PINNED");
    assert_eq!(
        pinned.first().map(|r| r.title.as_str()),
        Some("a pinned snippet")
    );
    let recent = rows(&snap, "RECENT");
    let kept = recent
        .iter()
        .find(|r| r.id.as_deref() == Some(KEPT))
        .ok_or("the kept item isn't listed")?;
    assert_eq!(kept.title, "hello from the demo");
    assert_eq!(
        kept.actions
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        ["copy", "pin", "delete"]
    );
    assert_eq!(
        rows(&snap, "SHORTCUT").first().map(|r| r.detail.as_str()),
        Some("No shortcut")
    );
    assert_eq!(
        snap.get(SLOT).map(|e| e.badge_summary.clone()),
        Some("Clips · 2 items".to_string())
    );

    // 3. Delete from Samo: gone from the page and from the history on disk.
    host.row_action(SLOT, KEPT, "delete");
    wait(&host, "the deleted row to go", 10, |s| {
        !rows(s, "RECENT")
            .iter()
            .any(|r| r.id.as_deref() == Some(KEPT))
    })?;
    let saved = fs::read_to_string(data.join("history.json")).map_err(io)?;
    assert!(!saved.contains(KEPT), "{saved}");
    assert!(saved.contains(PINNED));

    // 4. It declares its window, so R15 leaves it alone; two processes, the first and its worker.
    thread::sleep(Duration::from_secs(3));
    assert_eq!(
        host.snapshot().get(SLOT).map(|e| e.status.clone()),
        Some(Status::Running)
    );
    assert_eq!(processes(&root, "clips"), 2);

    // 5. Off leaves nothing running; uninstall deletes the history (R23).
    host.set_on(SLOT, false);
    let deadline = Instant::now() + Duration::from_secs(10);
    while processes(&root, "clips") > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        processes(&root, "clips"),
        0,
        "turning Clips off leaves nothing running"
    );
    host.uninstall("clips", false)
        .map_err(|e| format!("{e:?}"))?;
    assert!(!data.exists(), "uninstall deletes the history");
    host.shutdown_all();
    Ok(())
}
