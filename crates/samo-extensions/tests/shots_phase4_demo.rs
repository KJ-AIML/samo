//! ADR-004 Phase 4's demo, the parts a machine can check: the real `samo-shots` 0.4.0, installed from a store signed
//! with a test key, under Samo's real host. Stitching is covered by Shots' own tests on a synthetic page three
//! screens long (`StitcherTests`); watching a real scroll needs Screen Recording, so a person checks it (M19).
//!
//! Shots' settings are written first with no shortcuts, so the test takes no key from the Mac it runs on.

mod shots_common;

use samo_ext_protocol::Widget;
use samo_extensions::{ExtRoot, Host, HostConfig, Status};
use samo_testkit::store::{FakeStore, listing_for};
use shots_common::{
    Fallible, SLOT, build_shots, has_section, io, page, shots_processes, stage, version_at_least,
    wait,
};
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn shots_phase4_demo() -> Fallible<()> {
    let binary = build_shots()?;
    let dir = tempfile::tempdir().map_err(io)?;
    let home = dir.path().join("home");
    let root = ExtRoot::at(home.join("Library/Application Support/Samo/Extensions"));
    fs::create_dir_all(root.data_dir("shots")).map_err(io)?;
    fs::write(
        root.data_dir("shots").join("settings.json"),
        r#"{"after":"both","then":"edit","folder":"~/Pictures/Shots","format":"png","retina":true,"sound":false,"shortcuts":{}}"#,
    )
    .map_err(io)?;

    let shop = FakeStore::new()?;
    let manifest = stage(&dir.path().join("stage"), &binary, |t| t)?;
    assert!(
        version_at_least(&manifest.version, (0, 4, 0)),
        "scrolling came in 0.4.0"
    );
    assert!(
        manifest
            .windows
            .as_deref()
            .is_some_and(|w| w.contains("while you scroll")),
        "the store says it shows a frame and preview while you scroll: {:?}",
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

    // 1. Scrolling capture from Samo's page, and its shortcut row, empty until chosen.
    let widgets = page(&snap);
    assert!(
        widgets.iter().any(
            |w| matches!(w, Widget::Button { action, label, .. } if action == "scroll" && label == "Scrolling capture")
        ),
        "no Scrolling capture button"
    );
    let row = widgets
        .iter()
        .find_map(|w| match w {
            Widget::Rows { items } => items
                .iter()
                .find(|r| r.title == "Scrolling capture")
                .cloned(),
            _ => None,
        })
        .ok_or("no Scrolling capture shortcut row")?;
    assert_eq!(row.detail, "No shortcut");

    // 2. It stays up, then Off leaves nothing running.
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
