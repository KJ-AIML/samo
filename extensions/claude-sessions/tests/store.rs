//! Claude Sessions from the store (ADR-003 M3, RF1): replacing a dev build keeps it connected and keeps its data,
//! and uninstalling undoes Claude Code's settings before deleting anything. The real `samo-claude` runs in a
//! temporary home folder; the real `~/.claude` is never touched.

use samo_ext_protocol::{Manifest, Widget};
use samo_extensions::{
    ExtRoot, ExtSlot, ExtSnapshot, Host, HostConfig, Registry, Source, Status, StoreInstall,
};
use samo_testkit::store::{FakeStore, listing_for};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const SLOT: ExtSlot = ExtSlot(0);

type Fallible<T> = Result<T, String>;

fn io<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// `<dir>/claude-sessions/`: the manifest and a copy of the helper (a copy, not cargo's hard link: R5).
fn lay_out(dir: &Path) -> Fallible<PathBuf> {
    let ext = dir.join("claude-sessions");
    fs::create_dir_all(ext.join("bin")).map_err(io)?;
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/extension.toml"),
        ext.join("extension.toml"),
    )
    .map_err(io)?;
    let helper = ext.join("bin/samo-claude");
    fs::write(
        &helper,
        fs::read(env!("CARGO_BIN_EXE_samo-claude")).map_err(io)?,
    )
    .map_err(io)?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).map_err(io)?;
    Ok(ext)
}

struct World {
    dir: tempfile::TempDir,
    home: PathBuf,
    root: ExtRoot,
    settings: PathBuf,
    original: Value,
}

impl World {
    fn new() -> Fallible<Self> {
        let dir = tempfile::tempdir().map_err(io)?;
        let home = dir.path().join("home");
        let claude = home.join(".claude");
        fs::create_dir_all(claude.join("projects")).map_err(io)?;
        let original = json!({ "model": "opus", "permissions": { "allow": ["Bash(ls:*)"] } });
        let settings = claude.join("settings.json");
        fs::write(
            &settings,
            serde_json::to_string_pretty(&original).map_err(io)?,
        )
        .map_err(io)?;
        Ok(Self {
            root: ExtRoot::at(home.join("Library/Application Support/Samo/Extensions")),
            dir,
            home,
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

    /// The same helper, packed and pinned by a store.
    fn store_copy(&self, shop: &FakeStore) -> Fallible<(samo_domain::store::Listing, Vec<u8>)> {
        let stage = self.dir.path().join("stage");
        let ext = lay_out(&stage)?;
        let manifest =
            Manifest::parse(&fs::read_to_string(ext.join("extension.toml")).map_err(io)?)
                .map_err(io)?;
        let pkg = shop.put_package(&stage, "claude-sessions", &manifest.version)?;
        let bytes = fs::read(shop.dir.path().join(&pkg.file)).map_err(io)?;
        Ok((listing_for(&manifest, pkg), bytes))
    }

    fn settings(&self) -> Fallible<Value> {
        serde_json::from_slice(&fs::read(&self.settings).map_err(io)?).map_err(io)
    }
}

fn wait_for(
    host: &Host,
    what: &str,
    test: impl Fn(&ExtSnapshot) -> bool,
) -> Fallible<Arc<ExtSnapshot>> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let snap = host.snapshot();
        if test(&snap) {
            return Ok(snap);
        }
        if Instant::now() > deadline {
            return Err(format!(
                "timed out waiting for {what}; {:?}",
                snap.get(SLOT).map(|e| (&e.status, &e.note, e.dev))
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn connected(snap: &ExtSnapshot) -> bool {
    snap.get(SLOT).is_some_and(|e| {
        e.status == Status::Running
            && e.view
                .iter()
                .any(|w| matches!(w, Widget::Switch { key, on: true, .. } if key == "connect"))
    })
}

#[test]
fn replace_dev_keeps_connection_and_data() -> Fallible<()> {
    let w = World::new()?;
    let dev = lay_out(&w.dir.path().join("dev"))?;
    let host = w.host(Some(&dev));
    wait_for(&host, "the dev build to run", |s| {
        s.get(SLOT)
            .is_some_and(|e| e.status == Status::Running && !e.view.is_empty())
    })?;
    host.setting(SLOT, "connect", true);
    wait_for(&host, "Connect", connected)?;
    let hooked = w.settings()?;
    assert_ne!(hooked, w.original);
    let kept = w.root.data_dir("claude-sessions").join("kept-by-test");
    fs::write(&kept, "x").map_err(io)?;

    let shop = FakeStore::new()?;
    let (listing, bytes) = w.store_copy(&shop)?;
    let done = host.install_from_store(&listing, &bytes).map_err(io)?;
    assert!(matches!(done, StoreInstall::ReplacedDev { .. }), "{done:?}");

    // The signed copy runs, still connected: Claude Code's settings never changed, and its data is all there.
    wait_for(&host, "the signed copy, connected", |s| {
        connected(s) && s.get(SLOT).is_some_and(|e| !e.dev)
    })?;
    assert_eq!(w.settings()?, hooked);
    assert!(kept.is_file());
    let reg = Registry::load(&w.root).map_err(io)?;
    assert_eq!(
        reg.get("claude-sessions").map(|i| (i.source.clone(), i.on)),
        Some((Source::Catalog, true))
    );
    let history = fs::read_to_string(w.root.history_file()).map_err(io)?;
    assert!(history.contains("\"event\":\"replace-dev\""), "{history}");
    for never in ["\"event\":\"off\"", "\"event\":\"disconnect\""] {
        assert!(!history.contains(never), "{never} in {history}");
    }
    host.shutdown_all();
    Ok(())
}

#[test]
fn uninstall_undoes_settings_then_deletes() -> Fallible<()> {
    let w = World::new()?;
    let host = w.host(None);
    let shop = FakeStore::new()?;
    let (listing, bytes) = w.store_copy(&shop)?;
    host.install_from_store(&listing, &bytes).map_err(io)?;
    wait_for(&host, "it to run", |s| {
        s.get(SLOT)
            .is_some_and(|e| e.status == Status::Running && !e.view.is_empty())
    })?;
    host.setting(SLOT, "connect", true);
    wait_for(&host, "Connect", connected)?;
    assert_ne!(w.settings()?, w.original);

    host.uninstall("claude-sessions", false).map_err(io)?;
    // Claude Code's settings are as they were, the one-time backup is kept (and named on the page), and Samo's
    // copy is gone.
    assert_eq!(w.settings()?, w.original);
    assert!(w.home.join(".claude/settings.json.samo-bak").is_file());
    assert!(!w.root.ext_dir("claude-sessions").exists());
    assert!(
        Registry::load(&w.root)
            .map_err(io)?
            .get("claude-sessions")
            .is_none()
    );
    assert!(host.snapshot().get(SLOT).is_none());
    host.shutdown_all();
    Ok(())
}
