//! ADR-003 Phase 2's demo, end to end and automated: a store signed with a test key, served on this Mac, and
//! Samo's real store worker and extension host, running the real `samo-claude` and a stand-in for Hello.
//! Everything happens in a temporary home folder; the real `~/.claude` and caches are never touched.
//!
//! It follows the plan's Task 12 list: the store lists both; Install replaces the dev Claude Sessions, which stays
//! connected and then runs with no `SAMO_EXT_DEV_DIR`; Hello installs, updates and uninstalls, leaving nothing;
//! Claude Sessions' uninstall leaves `settings.json` clean and keeps the backup; a newer Samo is downloaded and
//! checked; without the server the store uses its cache; and nothing is fetched without a click. (The app swap
//! itself, and its restore on failure, are `samo-gpui`'s `update_swap_*` tests.)

use samo_domain::store::{Catalog, FileRef, Listing, SamoRelease};
use samo_ext_protocol::{Manifest, Widget};
use samo_extensions::net::UserAction;
use samo_extensions::store::Store;
use samo_extensions::worker::{SamoUpdate, StoreJob, StoreView, StoreWorker};
use samo_extensions::{ExtRoot, ExtSnapshot, Host, HostConfig, Registry, Source, Status};
use samo_testkit::store::{FakeStore, listing_for, png};
use serde_json::{Value, json};
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

const CS: &str = "claude-sessions";

/// `<dir>/claude-sessions/`: the real manifest and a copy of the helper (a copy, not cargo's hard link: R5).
fn claude_sessions(dir: &Path) -> Fallible<(PathBuf, Manifest)> {
    let ext = dir.join(CS);
    fs::create_dir_all(ext.join("bin")).map_err(io)?;
    let text =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/extension.toml")).map_err(io)?;
    fs::write(ext.join("extension.toml"), &text).map_err(io)?;
    let helper = ext.join("bin/samo-claude");
    fs::write(
        &helper,
        fs::read(env!("CARGO_BIN_EXE_samo-claude")).map_err(io)?,
    )
    .map_err(io)?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).map_err(io)?;
    Ok((ext, Manifest::parse(&text).map_err(io)?))
}

/// `<dir>/hello/`: a stand-in for Hello at `version` that says hello and waits.
fn hello(dir: &Path, version: &str) -> Fallible<Manifest> {
    let ext = dir.join("hello");
    fs::create_dir_all(ext.join("bin")).map_err(io)?;
    let text = format!(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"{version}\"\nprotocol = 1\nmin_samo = \"0.2.0\"\n\
         helper = \"bin/helper\"\nsummary = \"Says hello\"\n\n[[changes]]\nwhat = \"Nothing outside its own folder\"\n\
         where = \"~/Library/Application Support/Samo/Extensions/hello\"\nundo = \"Uninstall deletes the folder\"\n"
    );
    fs::write(ext.join("extension.toml"), &text).map_err(io)?;
    let helper = ext.join("bin/helper");
    fs::write(
        &helper,
        "#!/bin/sh\necho '{\"type\":\"hello\",\"protocol\":1}'\n\
         while read -r line; do case \"$line\" in *shutdown*) exit 0;; esac; done\n",
    )
    .map_err(io)?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).map_err(io)?;
    Manifest::parse(&text).map_err(io)
}

/// A tiny `Samo.app` at `version`, ad-hoc signed like Samo's own; zipped like a release when `zip` is set.
fn samo_app(dir: &Path, version: &str) -> Fallible<PathBuf> {
    let app = dir.join("Samo.app");
    fs::create_dir_all(app.join("Contents/MacOS")).map_err(io)?;
    fs::copy("/usr/bin/true", app.join("Contents/MacOS/samo-gpui")).map_err(io)?;
    fs::write(
        app.join("Contents/Info.plist"),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
             <key>CFBundleIdentifier</key><string>dev.talvyn.samo</string>\
             <key>CFBundleShortVersionString</key><string>{version}</string>\
             <key>CFBundleExecutable</key><string>samo-gpui</string>\
             <key>CFBundlePackageType</key><string>APPL</string></dict></plist>\n"
        ),
    )
    .map_err(io)?;
    let signed = Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .output()
        .map_err(io)?;
    if !signed.status.success() {
        return Err(String::from_utf8_lossy(&signed.stderr).into_owned());
    }
    Ok(app)
}

struct Demo {
    dir: tempfile::TempDir,
    home: PathBuf,
    shop: FakeStore,
    root: ExtRoot,
    settings: PathBuf,
    original: Value,
}

impl Demo {
    fn new() -> Fallible<Self> {
        let dir = tempfile::tempdir().map_err(io)?;
        let home = dir.path().join("home");
        fs::create_dir_all(home.join(".claude/projects")).map_err(io)?;
        let original = json!({ "model": "opus", "permissions": { "allow": ["Bash(ls:*)"] } });
        let settings = home.join(".claude/settings.json");
        fs::write(
            &settings,
            serde_json::to_string_pretty(&original).map_err(io)?,
        )
        .map_err(io)?;
        Ok(Self {
            root: ExtRoot::at(home.join("Library/Application Support/Samo/Extensions")),
            dir,
            home,
            shop: FakeStore::new()?,
            settings,
            original,
        })
    }

    fn host(&self, dev: Option<&Path>) -> Host {
        let mut config = HostConfig::new("0.2.0");
        config.dev_dir = dev.map(|d| d.to_string_lossy().into_owned());
        config.home = Some(self.home.clone());
        config.wake_every = Duration::from_millis(10);
        Host::start(self.root.clone(), config, Arc::new(|| {}))
    }

    /// Samo's store as the app makes it, in the temporary home; a new one is a restarted Samo.
    fn worker(&self, host: &Host) -> StoreWorker {
        let store = Store::new(
            &self.shop.catalog_url(),
            self.shop.public_key(),
            &self.home.join("Library/Caches/Samo/Store"),
            &self
                .home
                .join("Library/Application Support/Samo/store-state.toml"),
        );
        StoreWorker::start(store, host.clone(), Arc::new(|| {}))
    }

    fn package(&self, stage: &Path, manifest: &Manifest) -> Fallible<FileRef> {
        self.shop
            .put_package(stage, &manifest.id, &manifest.version)
    }

    fn settings(&self) -> Fallible<Value> {
        serde_json::from_slice(&fs::read(&self.settings).map_err(io)?).map_err(io)
    }
}

fn wait(what: &str, secs: u64, mut done: impl FnMut() -> bool) -> Fallible<()> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !done() {
        if Instant::now() > deadline {
            return Err(format!("timed out waiting for {what}"));
        }
        thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}

fn ext<'a>(snap: &'a ExtSnapshot, id: &str) -> Option<&'a samo_extensions::ExtView> {
    snap.extensions.iter().find(|e| e.id == id)
}

/// Running, from the dev folder or not; Claude Sessions also once it has drawn its page.
fn running(host: &Host, id: &str, dev: bool) -> bool {
    ext(&host.snapshot(), id).is_some_and(|e| {
        e.status == Status::Running && e.dev == dev && (id != CS || !e.view.is_empty())
    })
}

fn connected(host: &Host) -> bool {
    ext(&host.snapshot(), CS).is_some_and(|e| {
        e.status == Status::Running
            && e.view
                .iter()
                .any(|w| matches!(w, Widget::Switch { key, on: true, .. } if key == "connect"))
    })
}

fn idle_jobs(v: &StoreView) -> bool {
    v.jobs.is_empty() && !v.refreshing
}

fn catalog(at: &str, extensions: Vec<Listing>, samo: Option<SamoRelease>) -> Catalog {
    Catalog {
        schema: 1,
        published_at: at.into(),
        samo,
        extensions,
    }
}

#[test]
fn store_phase2_demo() -> Fallible<()> {
    let d = Demo::new()?;
    let stage = d.dir.path().join("stage");

    // The store: Claude Sessions and Hello 0.2.0 with icons, and Samo 0.3.0.
    let (_, cs_manifest) = claude_sessions(&stage)?;
    let cs_pkg = d.package(&stage, &cs_manifest)?;
    let hello_v1 = hello(&stage, "0.2.0")?;
    let hello_pkg = d.package(&stage, &hello_v1)?;
    let mut cs_listing = listing_for(&cs_manifest, cs_pkg);
    cs_listing.icon = Some(d.shop.put("claude-sessions-icon.png", &png(256, 256))?);
    let mut hello_listing = listing_for(&hello_v1, hello_pkg);
    hello_listing.icon = Some(d.shop.put("hello-icon.png", &png(128, 128))?);
    let new_samo = samo_app(&d.dir.path().join("build"), "0.3.0")?;
    let zip = d.dir.path().join("Samo-0.3.0-macos-arm64.zip");
    let zipped = Command::new("ditto")
        .args(["-c", "-k", "--keepParent"])
        .arg(&new_samo)
        .arg(&zip)
        .status()
        .map_err(io)?;
    assert!(zipped.success());
    let samo = SamoRelease {
        version: "0.3.0".into(),
        whats_new: "Faster".into(),
        min_macos: "11.0".into(),
        package: d
            .shop
            .put("Samo-0.3.0-macos-arm64.zip", &fs::read(&zip).map_err(io)?)?,
    };
    d.shop.publish(&catalog(
        "2026-11-02T10:00:00Z",
        vec![cs_listing.clone(), hello_listing.clone()],
        Some(samo.clone()),
    ))?;

    // Claude Sessions from a dev folder, connected: where KJ's Mac starts.
    let (dev, _) = claude_sessions(&d.dir.path().join("dev"))?;
    let host = d.host(Some(&dev));
    wait("the dev build to run", 15, || running(&host, CS, true))?;
    host.setting(samo_extensions::ExtSlot(0), "connect", true);
    wait("Connect", 15, || connected(&host))?;
    let hooked = d.settings()?;
    assert_ne!(hooked, d.original);

    // 7. Samo starting goes nowhere.
    let worker = d.worker(&host);
    thread::sleep(Duration::from_millis(300));
    assert!(d.shop.server.hits().is_empty(), "nothing before a click");

    // 1. A click opens the store: both listed, with icons.
    worker.submit(StoreJob::Refresh { force: false }, UserAction::clicked());
    wait("the catalog and icons", 10, || {
        let v = worker.view();
        v.catalog.is_some() && v.images.len() == 2 && idle_jobs(&v)
    })?;
    let ids: Vec<String> = worker
        .view()
        .catalog
        .map(|c| c.extensions.iter().map(|l| l.id.clone()).collect())
        .unwrap_or_default();
    assert_eq!(ids, [CS, "hello"]);

    // 2. Install replaces the dev build; it stays connected and keeps its data.
    worker.submit(StoreJob::Install(CS.into()), UserAction::clicked());
    wait("the signed copy", 20, || {
        running(&host, CS, false) && idle_jobs(&worker.view())
    })?;
    wait("it to be connected", 10, || connected(&host))?;
    assert_eq!(d.settings()?, hooked, "replacing never disconnects");
    host.shutdown_all();
    // Then without SAMO_EXT_DEV_DIR: it runs, signed, still connected.
    let host = d.host(None);
    let worker = d.worker(&host);
    wait("the signed copy without a dev folder", 15, || {
        running(&host, CS, false)
    })?;
    wait("it to be connected", 10, || connected(&host))?;
    assert_eq!(
        Registry::load(&d.root)
            .map_err(io)?
            .get(CS)
            .map(|i| i.source.clone()),
        Some(Source::Catalog)
    );

    // 3. Hello installs, updates once a newer version is published, and uninstalls, leaving nothing.
    wait("the cached catalog", 10, || worker.view().catalog.is_some())?;
    worker.submit(StoreJob::Install("hello".into()), UserAction::clicked());
    wait("Hello 0.2.0", 15, || {
        running(&host, "hello", false) && idle_jobs(&worker.view())
    })?;
    let hello_v2 = hello(&stage, "0.2.1")?;
    let mut hello_listing_v2 = listing_for(&hello_v2, d.package(&stage, &hello_v2)?);
    hello_listing_v2.icon = hello_listing.icon.clone();
    d.shop.publish(&catalog(
        "2026-11-03T10:00:00Z",
        vec![cs_listing.clone(), hello_listing_v2],
        Some(samo.clone()),
    ))?;
    worker.submit(StoreJob::Refresh { force: true }, UserAction::clicked());
    wait("the newer catalog", 10, || {
        worker
            .view()
            .catalog
            .is_some_and(|c| c.extensions.iter().any(|l| l.version == "0.2.1"))
    })?;
    worker.submit(StoreJob::Install("hello".into()), UserAction::clicked());
    wait("Hello 0.2.1", 15, || {
        ext(&host.snapshot(), "hello")
            .is_some_and(|e| e.version == "0.2.1" && e.status == Status::Running)
            && idle_jobs(&worker.view())
    })?;
    worker.submit(
        StoreJob::Uninstall {
            id: "hello".into(),
            anyway: false,
        },
        UserAction::clicked(),
    );
    wait("Hello to go", 15, || {
        ext(&host.snapshot(), "hello").is_none() && idle_jobs(&worker.view())
    })?;
    assert!(
        !d.root.ext_dir("hello").exists(),
        "nothing left under Extensions/hello"
    );

    // 4. Claude Sessions' uninstall leaves settings.json as it was, and keeps the one-time backup.
    worker.submit(
        StoreJob::Uninstall {
            id: CS.into(),
            anyway: false,
        },
        UserAction::clicked(),
    );
    wait("Claude Sessions to go", 15, || {
        ext(&host.snapshot(), CS).is_none() && idle_jobs(&worker.view())
    })?;
    assert_eq!(d.settings()?, d.original);
    assert!(d.home.join(".claude/settings.json.samo-bak").is_file());
    assert!(!d.root.ext_dir(CS).exists());

    // 5. A newer Samo is downloaded and checked, ready for --finish-update, before anything changes.
    let apps = d.dir.path().join("Applications");
    let old_samo = samo_app(&apps, "0.2.0")?;
    worker.submit(
        StoreJob::UpdateSamo {
            current_exe: old_samo.join("Contents/MacOS/samo-gpui"),
        },
        UserAction::clicked(),
    );
    wait("Samo 0.3.0 to be ready", 20, || {
        matches!(
            worker.view().samo,
            SamoUpdate::Ready(_) | SamoUpdate::Failed(_)
        )
    })?;
    let SamoUpdate::Ready(prepared) = worker.view().samo else {
        return Err(format!("not ready: {:?}", worker.view().samo));
    };
    assert_eq!(prepared.version, "0.3.0");
    assert!(
        prepared
            .app_path
            .starts_with(d.home.join("Library/Caches/Samo/Update/0.3.0"))
    );
    let version = Command::new("plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw"])
        .arg(old_samo.join("Contents/Info.plist"))
        .output()
        .map_err(io)?;
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "0.2.0",
        "the old app is untouched"
    );

    // 7, again: a restarted Samo asks for nothing, and shows its cache.
    let hits = d.shop.server.hits().len();
    host.shutdown_all();
    let host = d.host(None);
    let restarted = d.worker(&host);
    wait("the cache after a restart", 10, || {
        restarted.view().catalog.is_some()
    })?;
    thread::sleep(Duration::from_millis(300));
    assert_eq!(
        d.shop.server.hits().len(),
        hits,
        "{:?}",
        d.shop.server.hits()
    );

    // 6. With the store gone, a click finds it offline, and the cache is still shown.
    let Demo {
        dir: _dir, shop, ..
    } = d;
    drop(shop);
    restarted.submit(StoreJob::Refresh { force: true }, UserAction::clicked());
    wait("the offline note", 10, || {
        let v = restarted.view();
        v.offline.is_some() && !v.refreshing
    })?;
    let v = restarted.view();
    assert!(v.catalog.is_some_and(|c| c.extensions.len() == 2));
    assert!(v.fetched_at_ms.is_some());
    host.shutdown_all();
    Ok(())
}
