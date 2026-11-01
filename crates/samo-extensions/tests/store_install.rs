//! Installing, updating and uninstalling from a store on this Mac (ADR-003 M3), with a shell-script stand-in for
//! Hello's helper. Everything happens in a temporary home folder.

use samo_domain::extensions::{BackoffConfig, Source};
use samo_domain::store::StoreError;
use samo_ext_protocol::Manifest;
use samo_extensions::net::UserAction;
use samo_extensions::store::Store;
use samo_extensions::worker::{StoreJob, StoreWorker};
use samo_extensions::{
    ExtRoot, ExtSnapshot, Host, HostConfig, InstallError, Registry, Status, StoreInstall,
    UninstallFailed,
};
use samo_testkit::store::{FakeStore, catalog, listing_for};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type Fallible<T> = Result<T, String>;

fn io<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Says hello and waits; `off` and `uninstall` leave a mark in the (temporary) home folder.
fn helper_script(version: &str) -> String {
    format!(
        r#"case "$1" in
  off) touch "$HOME/off-ran"; exit 0;;
  uninstall) touch "$HOME/uninstall-ran"; exit 0;;
esac
touch "$SAMO_EXT_DATA_DIR/ran-{version}"
echo '{{"type":"hello","protocol":1}}'
while read -r line; do case "$line" in *shutdown*) exit 0;; esac; done"#
    )
}

fn manifest_text(version: &str, change: &str) -> String {
    format!(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"{version}\"\nprotocol = 1\nmin_samo = \"0.1.0\"\n\
         helper = \"bin/helper\"\nsummary = \"Says hello\"\n\n[[changes]]\nwhat = \"{change}\"\n\
         where = \"nowhere\"\nundo = \"nothing\"\n"
    )
}

/// Lays out `<dir>/hello/` with a manifest and a helper.
fn layout(dir: &Path, manifest: &str, script: &str) -> Fallible<Manifest> {
    let ext = dir.join("hello");
    let _ = fs::remove_dir_all(&ext);
    fs::create_dir_all(&ext).map_err(io)?;
    fs::write(ext.join("extension.toml"), manifest).map_err(io)?;
    samo_testkit::fake_helper(&ext.join("bin"), "helper", script).map_err(io)?;
    Manifest::parse(manifest).map_err(io)
}

struct World {
    dir: tempfile::TempDir,
    shop: FakeStore,
    root: ExtRoot,
}

impl World {
    fn new() -> Fallible<Self> {
        let dir = tempfile::tempdir().map_err(io)?;
        let root = ExtRoot::at(dir.path().join("home/Extensions"));
        fs::create_dir_all(dir.path().join("home")).map_err(io)?;
        Ok(Self {
            dir,
            shop: FakeStore::new()?,
            root,
        })
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn config(&self, dev: Option<&Path>) -> HostConfig {
        let mut c = HostConfig::new("0.2.0");
        c.dev_dir = dev.map(|d| d.to_string_lossy().into_owned());
        c.hello_timeout = Duration::from_secs(2);
        c.grace = Duration::from_millis(300);
        c.one_shot_timeout = Duration::from_secs(5);
        c.backoff = BackoffConfig {
            first_ms: 20,
            max_ms: 100,
            max_crashes: 3,
            window_ms: 60_000,
        };
        c.wake_every = Duration::from_millis(10);
        c.home = Some(self.home());
        c
    }

    fn host(&self, dev: Option<&Path>) -> Host {
        Host::start(self.root.clone(), self.config(dev), Arc::new(|| {}))
    }

    fn store(&self) -> Store {
        Store::new(
            &self.shop.catalog_url(),
            self.shop.public_key(),
            &self.home().join("Caches/Store"),
            &self.home().join("store-state.toml"),
        )
    }

    /// Packs Hello `version` into the store and returns its listing and package bytes.
    fn package(
        &self,
        version: &str,
        script: &str,
    ) -> Fallible<(samo_domain::store::Listing, Vec<u8>)> {
        let stage = self.dir.path().join(format!("stage-{version}"));
        let manifest = layout(&stage, &manifest_text(version, "Nothing"), script)?;
        let pkg = self.shop.put_package(&stage, "hello", version)?;
        let bytes = fs::read(self.shop.dir.path().join(&pkg.file)).map_err(io)?;
        Ok((listing_for(&manifest, pkg), bytes))
    }
}

fn wait_for(what: &str, secs: u64, mut done: impl FnMut() -> bool) -> Fallible<()> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !done() {
        if Instant::now() > deadline {
            return Err(format!("timed out waiting for {what}"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn hello(snap: &ExtSnapshot) -> Option<(String, bool, Status)> {
    snap.extensions
        .iter()
        .find(|e| e.id == "hello")
        .map(|e| (e.version.clone(), e.dev, e.status.clone()))
}

fn running(host: &Host, version: &str) -> bool {
    hello(&host.snapshot()) == Some((version.into(), false, Status::Running))
}

#[test]
fn install_update_uninstall_from_catalog() -> Fallible<()> {
    let w = World::new()?;
    let (l1, _) = w.package("0.1.0", &helper_script("0.1.0"))?;
    w.shop.publish(&catalog("2026-11-02T10:00:00Z", vec![l1]))?;
    let host = w.host(None);
    let worker = StoreWorker::start(w.store(), host.clone(), Arc::new(|| {}));

    // Install: from the catalog, through the worker, as the store's button does.
    worker.submit(StoreJob::Refresh { force: true }, UserAction::clicked());
    wait_for("the catalog", 10, || {
        let v = worker.view();
        v.catalog.is_some() && !v.refreshing
    })?;
    worker.submit(StoreJob::Install("hello".into()), UserAction::clicked());
    wait_for("0.1.0 to run", 10, || running(&host, "0.1.0"))?;
    wait_for("the job to finish", 5, || worker.view().jobs.is_empty())?;
    let data = w.root.data_dir("hello");
    assert!(data.join("ran-0.1.0").exists());
    let reg = Registry::load(&w.root).map_err(io)?;
    assert_eq!(
        reg.get("hello").map(|i| (&i.source, i.on)),
        Some((&Source::Catalog, true))
    );
    fs::write(data.join("kept"), "x").map_err(io)?;

    // Update: a newer catalog, a newer version beside the old, `current` swapped, data kept.
    let (l2, _) = w.package("0.2.0", &helper_script("0.2.0"))?;
    w.shop.publish(&catalog("2026-11-03T10:00:00Z", vec![l2]))?;
    worker.submit(StoreJob::Refresh { force: true }, UserAction::clicked());
    wait_for("the new catalog", 10, || {
        worker
            .view()
            .catalog
            .is_some_and(|c| c.extensions[0].version == "0.2.0")
    })?;
    worker.submit(StoreJob::Install("hello".into()), UserAction::clicked());
    wait_for("0.2.0 to run", 10, || running(&host, "0.2.0"))?;
    wait_for("the job to finish", 5, || worker.view().jobs.is_empty())?;
    assert!(data.join("ran-0.2.0").exists() && data.join("kept").exists());
    assert!(!w.root.version_dir("hello", "0.1.0").exists());
    assert_eq!(
        fs::read_link(w.root.current("hello")).map_err(io)?,
        PathBuf::from("0.2.0")
    );
    assert!(
        !w.home().join("off-ran").exists(),
        "an update never runs `off`"
    );

    // Uninstall: `off`, then `uninstall`, then the folder and the registry entry go.
    worker.submit(
        StoreJob::Uninstall {
            id: "hello".into(),
            anyway: false,
        },
        UserAction::clicked(),
    );
    wait_for("hello to go", 10, || hello(&host.snapshot()).is_none())?;
    assert!(w.home().join("off-ran").exists() && w.home().join("uninstall-ran").exists());
    assert!(!w.root.ext_dir("hello").exists());
    assert!(Registry::load(&w.root).map_err(io)?.get("hello").is_none());
    let history = fs::read_to_string(w.root.history_file()).map_err(io)?;
    for event in ["install", "update", "uninstall"] {
        assert!(
            history.contains(&format!("\"event\":\"{event}\"")),
            "{history}"
        );
    }
    host.shutdown_all();
    Ok(())
}

#[test]
fn update_goes_back_when_the_new_helper_is_silent() -> Fallible<()> {
    let w = World::new()?;
    let host = w.host(None);
    let (l1, b1) = w.package("0.1.0", &helper_script("0.1.0"))?;
    host.install_from_store(&l1, &b1).map_err(io)?;
    wait_for("0.1.0 to run", 10, || running(&host, "0.1.0"))?;

    let (l2, b2) = w.package("0.2.0", "exec sleep 30")?;
    let err = host
        .install_from_store(&l2, &b2)
        .map(|_| ())
        .map_err(|e| e.to_string());
    assert!(
        err.as_ref()
            .is_err_and(|e| e.contains("went back to 0.1.0")),
        "{err:?}"
    );
    wait_for("0.1.0 to run again", 10, || running(&host, "0.1.0"))?;
    assert_eq!(
        fs::read_link(w.root.current("hello")).map_err(io)?,
        PathBuf::from("0.1.0")
    );
    assert!(!w.root.version_dir("hello", "0.2.0").exists());
    assert_eq!(
        Registry::load(&w.root)
            .map_err(io)?
            .get("hello")
            .map(|i| i.version.clone()),
        Some("0.1.0".into())
    );
    host.shutdown_all();
    Ok(())
}

#[test]
fn dev_never_replaces_catalog() -> Fallible<()> {
    let w = World::new()?;
    let host = w.host(None);
    let (l1, b1) = w.package("0.1.0", &helper_script("0.1.0"))?;
    assert_eq!(
        host.install_from_store(&l1, &b1).map_err(io)?,
        StoreInstall::Installed {
            version: "0.1.0".into()
        }
    );
    host.shutdown_all();

    // A dev build of the same extension, newer even, is skipped and the page says so.
    let dev = w.dir.path().join("dev");
    layout(
        &dev,
        &manifest_text("0.3.0", "Nothing"),
        &helper_script("0.3.0"),
    )?;
    let host = w.host(Some(&dev));
    wait_for("the signed copy to run", 10, || running(&host, "0.1.0"))?;
    let snap = host.snapshot();
    assert!(
        snap.problems
            .iter()
            .any(|p| p.contains("installed from the store")),
        "{:?}",
        snap.problems
    );
    assert_eq!(
        Registry::load(&w.root)
            .map_err(io)?
            .get("hello")
            .map(|i| i.source.clone()),
        Some(Source::Catalog)
    );
    host.shutdown_all();
    Ok(())
}

#[test]
fn blocked_dev_uninstall_is_a_failed_step() -> Fallible<()> {
    let w = World::new()?;
    let dev = w.dir.path().join("dev");
    layout(
        &dev,
        &manifest_text("0.1.0", "Nothing"),
        &helper_script("0.1.0"),
    )?;
    let host = w.host(Some(&dev));
    wait_for("the dev build to run", 10, || {
        hello(&host.snapshot()).is_some_and(|h| h.2 == Status::Running)
    })?;
    host.shutdown_all();

    // Without SAMO_EXT_DEV_DIR it can't run, so it can't undo anything: that's a failure, not a quiet success.
    let host = w.host(None);
    assert_eq!(
        host.uninstall("hello", false),
        Err(UninstallFailed::CantRun)
    );
    assert!(w.root.ext_dir("hello").exists());
    assert!(hello(&host.snapshot()).is_some());
    assert!(!w.home().join("uninstall-ran").exists());

    // Uninstall anyway.
    host.uninstall("hello", true).map_err(io)?;
    assert!(!w.root.ext_dir("hello").exists());
    assert!(hello(&host.snapshot()).is_none());
    let history = fs::read_to_string(w.root.history_file()).map_err(io)?;
    assert!(
        history.contains("uninstall-failed") && history.contains("anyway"),
        "{history}"
    );
    host.shutdown_all();
    Ok(())
}

#[test]
fn manifest_must_match_catalog_entry() -> Fallible<()> {
    let w = World::new()?;
    let host = w.host(None);
    let (listing, bytes) = w.package("0.1.0", &helper_script("0.1.0"))?;

    // The catalog says it changes nothing; the package says otherwise.
    let stage = w.dir.path().join("stage-liar");
    layout(
        &stage,
        &manifest_text("0.1.0", "Reads your mail"),
        &helper_script("0.1.0"),
    )?;
    let liar = w.shop.put_package(&stage, "hello", "0.1.0")?;
    let liar_bytes = fs::read(w.shop.dir.path().join(&liar.file)).map_err(io)?;
    let mut claims = listing.clone();
    claims.package = liar;
    assert!(matches!(
        host.install_from_store(&claims, &liar_bytes),
        Err(InstallError::Store(StoreError::ManifestMismatch("changes")))
    ));
    let mut wrong_version = listing.clone();
    wrong_version.version = "0.1.1".into();
    assert!(matches!(
        host.install_from_store(&wrong_version, &bytes),
        Err(InstallError::Store(StoreError::ManifestMismatch(_)))
    ));
    assert!(hello(&host.snapshot()).is_none());
    assert!(Registry::load(&w.root).map_err(io)?.get("hello").is_none());
    let leftovers: Vec<_> = fs::read_dir(w.root.ext_dir("hello"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    host.shutdown_all();
    Ok(())
}
