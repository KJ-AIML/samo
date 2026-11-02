//! ADR-004 Phase 2's demo, the parts a machine can check: the real `samo-shots` 0.2.0, installed from a store signed
//! with a test key, under Samo's real host. Drawing in the editor is driven headless by Shots' own tests
//! (`EditorWindowTests`); the rest needs a person (M8–M13).
//!
//! Shots' settings are written first with no shortcuts, so the test never takes ⇧⌘1 or ⇧⌘2 from the Mac it runs on.

mod shots_common;

use samo_ext_protocol::Widget;
use samo_extensions::windows::{SystemWindows, WindowOwners};
use samo_extensions::{ExtRoot, Host, HostConfig, Status};
use samo_testkit::store::{FakeStore, listing_for};
use shots_common::{
    Fallible, SLOT, build_shots, chosen, has_section, io, page, shots_processes, stage,
    version_at_least, wait,
};
use std::fs;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// The pids of the processes running the installed helper.
fn shots_pids(root: &ExtRoot) -> Vec<u32> {
    let path = root.ext_dir("shots").to_string_lossy().into_owned();
    Command::new("pgrep")
        .args(["-f", &path])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The buttons on the recent row for `path`.
fn recent_buttons(snap: &samo_extensions::ExtSnapshot, path: &str) -> Option<Vec<String>> {
    page(snap).into_iter().find_map(|w| match w {
        Widget::Rows { items } => items
            .into_iter()
            .find(|r| r.id.as_deref() == Some(path))
            .map(|r| r.actions.into_iter().map(|a| a.id).collect()),
        _ => None,
    })
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend(u32::try_from(data.len()).unwrap_or(0).to_be_bytes());
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend(kind);
    out.extend(data);
    out.extend(crc.sum().to_be_bytes());
}

/// A real PNG, 8 × 6 pixels of colour.
fn small_png() -> Fallible<Vec<u8>> {
    let (width, height) = (8u8, 6u8);
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            raw.extend([200, 80 + x * 10, 40 + y * 20]);
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw).map_err(io)?;
    let pixels = z.finish().map_err(io)?;
    let mut header = Vec::new();
    header.extend(u32::from(width).to_be_bytes());
    header.extend(u32::from(height).to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &pixels);
    chunk(&mut png, b"IEND", &[]);
    Ok(png)
}

#[test]
fn shots_phase2_demo() -> Fallible<()> {
    let binary = build_shots()?;
    let dir = tempfile::tempdir().map_err(io)?;
    let home = dir.path().join("home");
    let root = ExtRoot::at(home.join("Library/Application Support/Samo/Extensions"));
    let data = root.data_dir("shots");
    fs::create_dir_all(&data).map_err(io)?;
    // Settings as Phase 1 wrote them (no `then`), with no shortcuts.
    fs::write(
        data.join("settings.json"),
        r#"{"after":"both","folder":"~/Pictures/Shots","format":"png","retina":true,"sound":false,"shortcuts":{}}"#,
    )
    .map_err(io)?;
    // A shot saved in this home's Pictures, not the account's, and listed as recent.
    let shots = home.join("Pictures/Shots");
    fs::create_dir_all(&shots).map_err(io)?;
    let shot = shots.join("Shot 2026-11-02 at 01.00.00.png");
    let png = small_png()?;
    fs::write(&shot, &png).map_err(io)?;
    let shot_path = shot.to_string_lossy().into_owned();
    fs::write(
        data.join("recent.json"),
        format!(
            r#"{{"shots":[{{"path":{},"bytes":{},"at":815245682}}]}}"#,
            serde_json::to_string(&shot_path).map_err(io)?,
            png.len()
        ),
    )
    .map_err(io)?;
    // A file a drag needed last time.
    fs::create_dir_all(data.join("drag")).map_err(io)?;
    fs::write(data.join("drag/left over.png"), b"x").map_err(io)?;

    let shop = FakeStore::new()?;
    let manifest = stage(&dir.path().join("stage"), &binary, |t| t)?;
    assert!(
        version_at_least(&manifest.version, (0, 2, 0)),
        "the editor came in 0.2.0"
    );
    assert!(
        manifest
            .windows
            .as_deref()
            .is_some_and(|w| w.contains("the editor")),
        "the store says it opens an editor: {:?}",
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
        s.get(SLOT).is_some_and(|e| e.status == Status::Running)
            && has_section(s, "AFTER A CAPTURE")
    })?;

    // 1. First and Then: Phase 1's settings load as both, then the editor.
    assert_eq!(chosen(&snap, "after").as_deref(), Some("both"));
    assert_eq!(chosen(&snap, "then").as_deref(), Some("edit"));
    host.choice(SLOT, "then", "nothing");
    wait(&host, "then: nothing", 10, |s| {
        chosen(s, "then").as_deref() == Some("nothing")
    })?;
    host.choice(SLOT, "then", "edit");
    wait(&host, "then: edit", 10, |s| {
        chosen(s, "then").as_deref() == Some("edit")
    })?;

    // 2. The shot in this home's Pictures shows in Recent, with Edit beside Copy, Reveal and Pin.
    let snap = wait(&host, "the recent shot", 10, |s| {
        recent_buttons(s, &shot_path).is_some()
    })?;
    assert_eq!(
        recent_buttons(&snap, &shot_path),
        Some(vec![
            "copy".to_string(),
            "reveal".into(),
            "pin".into(),
            "edit".into()
        ])
    );
    // The drag folder was emptied at start.
    assert!(
        !data.join("drag").exists(),
        "files a drag needed are never left behind"
    );

    // 3. Edit opens the editor on that shot: a window Shots declared, so R15 leaves it be.
    let pids = shots_pids(&root);
    assert_eq!(pids.len(), 2, "the first process and its worker");
    let before = SystemWindows.count(&pids).unwrap_or(0);
    host.row_action(SLOT, &shot_path, "edit");
    let deadline = Instant::now() + Duration::from_secs(10);
    while SystemWindows.count(&pids).unwrap_or(0) <= before && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    assert!(
        SystemWindows.count(&pids).unwrap_or(0) > before,
        "the editor opened"
    );
    thread::sleep(Duration::from_secs(3));
    assert_eq!(
        host.snapshot().get(SLOT).map(|e| e.status.clone()),
        Some(Status::Running)
    );

    // 4. Off: the editor closes with Shots, nothing is left running, the shot stays where it was saved.
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
    assert!(shot.exists());
    assert!(!data.join("drag").exists());
    host.shutdown_all();
    Ok(())
}
