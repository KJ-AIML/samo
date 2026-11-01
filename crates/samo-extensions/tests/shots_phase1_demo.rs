//! ADR-004 Phase 1's demo, the parts a machine can check: the real `samo-shots`, built from `extensions/shots`,
//! installed from a store signed with a test key, running under Samo's real host and the real window server (R15).
//! Capturing needs Screen Recording, which only a person can allow; that part is KJ's checks by hand (M1–M7).
//!
//! Shots' settings are written first with no shortcuts, so the test never takes ⇧⌘1 or ⇧⌘2 from the Mac it runs on.

use samo_ext_protocol::{Manifest, Widget};
use samo_extensions::{ExtRoot, ExtSlot, ExtSnapshot, Host, HostConfig, Status};
use samo_testkit::store::{FakeStore, listing_for};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type Fallible<T> = Result<T, String>;

fn io<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

const SLOT: ExtSlot = ExtSlot(0);

fn shots_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/shots")
}

/// Builds `samo-shots` (debug) and returns where it is.
fn build_shots() -> Fallible<PathBuf> {
    let dir = shots_dir();
    let ok = Command::new("swift")
        .args(["build", "--package-path"])
        .arg(&dir)
        .args(["--product", "samo-shots"])
        .status()
        .map_err(|e| format!("Shots needs Swift ({e})"))?;
    if !ok.success() {
        return Err("swift build failed".into());
    }
    let out = Command::new("swift")
        .args(["build", "--package-path"])
        .arg(&dir)
        .arg("--show-bin-path")
        .output()
        .map_err(io)?;
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()).join("samo-shots"))
}

/// `<stage>/shots/`: the manifest (changed by `edit`), the helper and the fonts, as `ext-package` lays them out.
fn stage(stage: &Path, binary: &Path, edit: impl Fn(String) -> String) -> Fallible<Manifest> {
    let ext = stage.join("shots");
    let _ = fs::remove_dir_all(&ext);
    fs::create_dir_all(ext.join("bin")).map_err(io)?;
    fs::create_dir_all(ext.join("share/fonts")).map_err(io)?;
    let text = edit(fs::read_to_string(shots_dir().join("extension.toml")).map_err(io)?);
    fs::write(ext.join("extension.toml"), &text).map_err(io)?;
    let helper = ext.join("bin/samo-shots");
    fs::write(&helper, fs::read(binary).map_err(io)?).map_err(io)?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).map_err(io)?;
    for font in fs::read_dir(shots_dir().join("share/fonts")).map_err(io)? {
        let font = font.map_err(io)?;
        fs::write(
            ext.join("share/fonts").join(font.file_name()),
            fs::read(font.path()).map_err(io)?,
        )
        .map_err(io)?;
    }
    Manifest::parse(&text).map_err(io)
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

fn has_section(snap: &ExtSnapshot, title: &str) -> bool {
    page(snap)
        .iter()
        .any(|w| matches!(w, Widget::Section { title: t, .. } if t == title))
}

fn chosen(snap: &ExtSnapshot, key: &str) -> Option<String> {
    page(snap).into_iter().find_map(|w| match w {
        Widget::Choice {
            key: k, selected, ..
        } if k == key => Some(selected),
        _ => None,
    })
}

/// Processes running the installed helper (the first process and its worker).
fn shots_processes(root: &ExtRoot) -> usize {
    let path = root.ext_dir("shots").to_string_lossy().into_owned();
    Command::new("pgrep")
        .args(["-f", &path])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0)
}

#[test]
fn shots_phase1_demo() -> Fallible<()> {
    let binary = build_shots()?;
    let dir = tempfile::tempdir().map_err(io)?;
    let home = dir.path().join("home");
    let root = ExtRoot::at(home.join("Library/Application Support/Samo/Extensions"));
    // No shortcuts: the test must not take ⇧⌘1 or ⇧⌘2 from this Mac.
    fs::create_dir_all(root.data_dir("shots")).map_err(io)?;
    fs::write(
        root.data_dir("shots").join("settings.json"),
        r#"{"after":"both","folder":"~/Pictures/Shots","format":"png","retina":true,"sound":false,"shortcuts":{}}"#,
    )
    .map_err(io)?;

    let shop = FakeStore::new()?;
    let manifest = stage(&dir.path().join("stage"), &binary, |t| t)?;
    let package = shop.put_package(&dir.path().join("stage"), "shots", &manifest.version)?;
    let bytes = fs::read(shop.dir.path().join(&package.file)).map_err(io)?;
    let listing = listing_for(&manifest, package);
    // 1. What it may do travels with the listing.
    assert!(listing.windows.is_some());
    assert_eq!(listing.permissions, ["screen-recording"]);
    assert_eq!(listing.min_macos.as_deref(), Some("14.0"));

    let mut config = HostConfig::new("0.3.0");
    config.home = Some(home.clone());
    config.wake_every = Duration::from_millis(10);
    let host = Host::start(root.clone(), config.clone(), Arc::new(|| {}));
    host.install_from_store(&listing, &bytes).map_err(io)?;

    // 2. It runs, says hello, and draws its page.
    let snap = wait(&host, "its page", 20, |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Running) && has_section(s, "SHORTCUTS")
    })?;
    for title in [
        "SCREEN RECORDING",
        "CAPTURE",
        "RECENT",
        "SHORTCUTS",
        "AFTER A CAPTURE",
    ] {
        assert!(has_section(&snap, title), "no {title}");
    }
    assert_eq!(
        snap.get(SLOT).map(|e| e.permissions.clone()),
        Some(vec!["screen-recording".to_string()])
    );

    // 3. It declares windows, so R15 leaves it alone, check after check.
    thread::sleep(Duration::from_secs(5));
    assert_eq!(
        host.snapshot().get(SLOT).map(|e| e.status.clone()),
        Some(Status::Running)
    );
    assert_eq!(
        shots_processes(&root),
        2,
        "the first process and its worker"
    );

    // 4. A choice from Samo changes its settings and its page.
    host.choice(SLOT, "then", "pin");
    wait(&host, "the choice", 10, |s| {
        chosen(s, "then").as_deref() == Some("pin")
    })?;
    let saved = fs::read_to_string(root.data_dir("shots").join("settings.json")).map_err(io)?;
    assert!(saved.contains(r#""then":"pin""#), "{saved}");

    // 5. Off: the helper and its worker are gone.
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

    // 6. The same helper, its manifest without `windows`: once it opens a window, R15 stops it.
    let home2 = dir.path().join("home2");
    let root2 = ExtRoot::at(home2.join("Library/Application Support/Samo/Extensions"));
    fs::create_dir_all(root2.data_dir("shots")).map_err(io)?;
    fs::write(
        root2.data_dir("shots").join("settings.json"),
        r#"{"after":"both","folder":"~/Pictures/Shots","format":"png","retina":true,"sound":false,"shortcuts":{}}"#,
    )
    .map_err(io)?;
    let undeclared = stage(&dir.path().join("stage2"), &binary, |t| {
        t.lines()
            .filter(|l| !l.starts_with("windows ="))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    assert!(undeclared.windows.is_none());
    let package = shop.put_package(&dir.path().join("stage2"), "shots", &undeclared.version)?;
    let bytes = fs::read(shop.dir.path().join(&package.file)).map_err(io)?;
    let mut config2 = config;
    config2.home = Some(home2);
    let host = Host::start(root2.clone(), config2, Arc::new(|| {}));
    host.install_from_store(&listing_for(&undeclared, package), &bytes)
        .map_err(io)?;
    wait(&host, "the undeclared copy to run", 20, |s| {
        s.get(SLOT).is_some_and(|e| e.status == Status::Running) && has_section(s, "SHORTCUTS")
    })?;
    host.action(SLOT, "change-shortcuts");
    let snap = wait(&host, "R15 to stop it", 15, |s| {
        matches!(s.get(SLOT).map(|e| &e.status), Some(Status::Stopped(_)))
    })?;
    assert_eq!(
        snap.get(SLOT).map(|e| e.status.clone()),
        Some(Status::Stopped("Opened windows it didn't declare".into()))
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while shots_processes(&root2) > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(shots_processes(&root2), 0, "R15 ends the whole tree");
    host.shutdown_all();
    Ok(())
}
