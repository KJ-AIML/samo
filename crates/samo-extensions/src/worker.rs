//! The store's worker (R13): one thread runs every store job, one at a time (fetching the catalog and images,
//! installing, updating, uninstalling), so the window never waits on the network or a helper. Each job carries
//! the click that asked for it (R12). The UI reads [`StoreView`] and is woken when it changes.

use crate::Host;
use crate::net::UserAction;
use crate::store::{ImageKind, Store};
use crate::update::{PreparedUpdate, can_update, prepare_update, staging_root};
use samo_domain::store::{Catalog, FileRef, PACKAGE_CAP, newer};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

pub enum StoreJob {
    /// The catalog (if the cached one is over 10 minutes old, or `force`), then any icons not cached yet.
    Refresh {
        force: bool,
    },
    /// One detail page's screenshots.
    Screenshots(Vec<FileRef>),
    /// Installs, updates, or replaces a dev build with, the catalog's version of `id`.
    Install(String),
    Uninstall {
        id: String,
        anyway: bool,
    },
    /// Downloads and checks the catalog's Samo, to replace the one running from `current_exe`.
    UpdateSamo {
        current_exe: PathBuf,
    },
}

/// Samo's own update, as Settings shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SamoUpdate {
    #[default]
    Idle,
    Preparing,
    /// Checked and ready: Samo starts the new app's `--finish-update` and quits.
    Ready(PreparedUpdate),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Installing,
    Updating,
    Uninstalling,
}

/// An install, update or uninstall in progress, or how it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobView {
    pub kind: JobKind,
    /// Set once it has failed, in words that say what to do.
    pub error: Option<String>,
    /// An uninstall that stopped before deleting can be done anyway.
    pub can_force: bool,
}

/// A verified image's bytes, or why it was refused.
pub type ImageResult = Result<Arc<Vec<u8>>, String>;

/// What the store screens draw.
#[derive(Debug, Clone, Default)]
pub struct StoreView {
    /// The last verified catalog: fetched now, or from the cache.
    pub catalog: Option<Arc<Catalog>>,
    pub fetched_at_ms: Option<i64>,
    pub refreshing: bool,
    /// Why the last fetch failed; the catalog shown, if any, is the cached one.
    pub offline: Option<String>,
    /// Icons and screenshots by SHA-256. Missing means not loaded (yet).
    pub images: Arc<HashMap<String, ImageResult>>,
    /// By extension id.
    pub jobs: BTreeMap<String, JobView>,
    pub samo: SamoUpdate,
}

struct Shared {
    view: Mutex<StoreView>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn view(&self) -> MutexGuard<'_, StoreView> {
        self.view.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn change(&self, f: impl FnOnce(&mut StoreView)) {
        f(&mut self.view());
        (self.wake)();
    }

    fn set_image(&self, sha: &str, result: ImageResult) {
        self.change(|v| {
            Arc::make_mut(&mut v.images).insert(sha.to_string(), result);
        });
    }
}

/// Samo's handle on the store. Cheap to clone.
#[derive(Clone)]
pub struct StoreWorker {
    tx: Sender<(StoreJob, UserAction)>,
    shared: Arc<Shared>,
}

impl StoreWorker {
    /// Starts the worker. It first shows what the cache holds (the catalog and its icons), without going online.
    pub fn start(store: Store, host: Host, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel::<(StoreJob, UserAction)>();
        let shared = Arc::new(Shared {
            view: Mutex::new(StoreView::default()),
            wake,
        });
        let s = Arc::clone(&shared);
        let _ = thread::Builder::new()
            .name("samo-store".into())
            .spawn(move || {
                load_cache(&store, &s);
                for (job, action) in rx {
                    run(&store, &host, &s, job, &action);
                }
            });
        Self { tx, shared }
    }

    /// Queues `job`, spending the click that asked for it.
    pub fn submit(&self, job: StoreJob, action: UserAction) {
        if let StoreJob::Refresh { .. } = job {
            self.shared.change(|v| v.refreshing = true);
        }
        let _ = self.tx.send((job, action));
    }

    pub fn view(&self) -> StoreView {
        self.shared.view().clone()
    }

    /// A page closed: its screenshots' bytes go (icons stay; they are small).
    pub fn forget_images(&self, shas: &[String]) {
        self.shared.change(|v| {
            let images = Arc::make_mut(&mut v.images);
            for sha in shas {
                images.remove(sha);
            }
        });
    }

    /// The user dismissed a failed job's message.
    pub fn dismiss(&self, id: &str) {
        self.shared.change(|v| {
            if v.jobs.get(id).is_some_and(|j| j.error.is_some()) {
                v.jobs.remove(id);
            }
        });
    }
}

fn load_cache(store: &Store, s: &Shared) {
    let Some((catalog, at)) = store.cached() else {
        return;
    };
    let mut icons = HashMap::new();
    for icon in catalog.extensions.iter().filter_map(|l| l.icon.as_ref()) {
        if let Some(bytes) = store.cached_image(icon, ImageKind::Icon) {
            icons.insert(icon.sha256.clone(), Ok(Arc::new(bytes)));
        }
    }
    s.change(|v| {
        v.catalog = Some(catalog);
        v.fetched_at_ms = Some(at);
        Arc::make_mut(&mut v.images).extend(icons);
    });
}

fn run(store: &Store, host: &Host, s: &Shared, job: StoreJob, action: &UserAction) {
    match job {
        StoreJob::Refresh { force } => {
            match store.refresh(action, force) {
                Ok((catalog, at)) => s.change(|v| {
                    v.catalog = Some(Arc::clone(&catalog));
                    v.fetched_at_ms = Some(at);
                    v.offline = None;
                }),
                Err(e) => s.change(|v| v.offline = Some(e.to_string())),
            }
            let catalog = s.view().catalog.clone();
            if let Some(catalog) = catalog {
                for icon in catalog.extensions.iter().filter_map(|l| l.icon.as_ref()) {
                    if s.view().images.contains_key(&icon.sha256) {
                        continue;
                    }
                    let got = store.image(icon, ImageKind::Icon, action);
                    s.set_image(&icon.sha256, got.map(Arc::new).map_err(|e| e.to_string()));
                }
            }
            s.change(|v| v.refreshing = false);
        }
        StoreJob::Screenshots(refs) => {
            for r in refs {
                if s.view().images.get(&r.sha256).is_some_and(Result::is_ok) {
                    continue;
                }
                let got = store.image(&r, ImageKind::Screenshot, action);
                s.set_image(&r.sha256, got.map(Arc::new).map_err(|e| e.to_string()));
            }
        }
        StoreJob::Install(id) => install(store, host, s, &id, action),
        StoreJob::UpdateSamo { current_exe } => {
            s.change(|v| v.samo = SamoUpdate::Preparing);
            let result = update_samo(store, s, &current_exe, action);
            s.change(|v| {
                v.samo = match result {
                    Ok(ready) => SamoUpdate::Ready(ready),
                    Err(why) => SamoUpdate::Failed(why),
                }
            });
        }
        StoreJob::Uninstall { id, anyway } => {
            s.change(|v| {
                v.jobs.insert(
                    id.clone(),
                    JobView {
                        kind: JobKind::Uninstalling,
                        error: None,
                        can_force: false,
                    },
                );
            });
            let done = host.uninstall(&id, anyway);
            s.change(|v| match done {
                Ok(()) => {
                    v.jobs.remove(&id);
                }
                Err(e) => {
                    let can_force = !matches!(e, crate::UninstallFailed::NotInstalled);
                    v.jobs.insert(
                        id.clone(),
                        JobView {
                            kind: JobKind::Uninstalling,
                            error: Some(format!("Couldn't uninstall: {e}.")),
                            can_force,
                        },
                    );
                }
            });
        }
    }
}

fn install(store: &Store, host: &Host, s: &Shared, id: &str, action: &UserAction) {
    let listing = s
        .view()
        .catalog
        .as_ref()
        .and_then(|c| c.extensions.iter().find(|l| l.id == id).cloned());
    let installed = host
        .snapshot()
        .extensions
        .iter()
        .find(|e| e.id == id)
        .map(|e| (e.version.clone(), e.dev));
    let kind = if installed.is_some() {
        JobKind::Updating
    } else {
        JobKind::Installing
    };
    let fail = |why: String| {
        s.change(|v| {
            v.jobs.insert(
                id.to_string(),
                JobView {
                    kind,
                    error: Some(why),
                    can_force: false,
                },
            );
        });
    };
    let Some(listing) = listing else {
        fail("The store no longer lists it. Refresh the store and try again.".into());
        return;
    };
    if installed
        .as_ref()
        .is_some_and(|(v, dev)| !dev && *v == listing.version)
    {
        s.change(|v| {
            v.jobs.remove(id);
        });
        return;
    }
    s.change(|v| {
        v.jobs.insert(
            id.to_string(),
            JobView {
                kind,
                error: None,
                can_force: false,
            },
        );
    });
    let bytes = match store.file(&listing.name, &listing.package, PACKAGE_CAP, action) {
        Ok(b) => b,
        Err(e) => {
            fail(format!(
                "Couldn't download {}: {e}. Try again.",
                listing.name
            ));
            return;
        }
    };
    match host.install_from_store(&listing, &bytes) {
        Ok(_) => s.change(|v| {
            v.jobs.remove(id);
        }),
        Err(e) => fail(format!("Couldn't install {}: {e}.", listing.name)),
    }
}

fn update_samo(
    store: &Store,
    s: &Shared,
    current_exe: &std::path::Path,
    action: &UserAction,
) -> Result<PreparedUpdate, String> {
    can_update(current_exe).map_err(|e| e.to_string())?;
    let release = s
        .view()
        .catalog
        .as_ref()
        .and_then(|c| c.samo.clone())
        .ok_or("The store doesn't offer a Samo download right now. Check again later")?;
    if !newer(&release.version, env!("CARGO_PKG_VERSION")) {
        return Err("You have the latest Samo".into());
    }
    let root = staging_root().ok_or("Samo can't find your home folder")?;
    prepare_update(store, &release, action, &root)
        .map_err(|e| format!("Couldn't get Samo {}: {e}", release.version))
}
