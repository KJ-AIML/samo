//! The host: which extensions are installed, their helpers' state, and what the UI may ask of them.

use crate::limits::Waker;
use crate::state::{ExtSlot, ExtSnapshot, ExtView, PendingPopup, Status};
use crate::supervisor::{self, Event, one_shot};
use crate::{ExtRoot, History, Installed, Registry, find_dev_packages, install_dev};
use samo_domain::extensions::{BackoffConfig, Source, may_run, samo_satisfies};
use samo_ext_protocol::{BadgeMeter, HostMsg, Manifest, Widget};
use std::collections::VecDeque;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// At most this many extensions are loaded.
pub const MAX_EXTENSIONS: usize = 16;
/// At most this many pop-ups wait for the UI; older ones are dropped.
const MAX_QUEUED_POPUPS: usize = 10;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub samo_version: String,
    /// `SAMO_EXT_DEV_DIR`: install unsigned extensions from this folder, and let them run.
    pub dev_dir: Option<String>,
    pub backoff: BackoffConfig,
    /// A helper must say `hello` within this.
    pub hello_timeout: Duration,
    /// After `shutdown`, how long a helper has before it is killed.
    pub grace: Duration,
    /// How long `<helper> on` / `off` may take.
    pub one_shot_timeout: Duration,
    /// How often a helper's CPU and memory are checked, and how many checks are averaged.
    pub resource_every: Duration,
    pub cpu_samples: usize,
    pub cpu_limit_pct: f32,
    pub rss_limit_bytes: u64,
    /// The UI is woken at most once per this.
    pub wake_every: Duration,
    /// The home folder helpers are given; `None` passes on Samo's own. Tests point it at a temporary folder so
    /// a helper never touches the real `~`.
    pub home: Option<PathBuf>,
}

impl HostConfig {
    /// ADR-002's limits, with `SAMO_EXT_DEV_DIR` from the environment.
    pub fn new(samo_version: impl Into<String>) -> Self {
        Self {
            samo_version: samo_version.into(),
            dev_dir: env::var("SAMO_EXT_DEV_DIR").ok().filter(|d| !d.is_empty()),
            backoff: BackoffConfig::default(),
            hello_timeout: Duration::from_secs(5),
            grace: Duration::from_secs(2),
            one_shot_timeout: Duration::from_secs(10),
            resource_every: Duration::from_secs(10),
            cpu_samples: 6,
            cpu_limit_pct: 10.0,
            rss_limit_bytes: 150 * 1024 * 1024,
            wake_every: Duration::from_millis(250),
            home: None,
        }
    }
}

pub(crate) struct Inner {
    pub root: ExtRoot,
    pub config: HostConfig,
    pub state: Mutex<HostState>,
    pub waker: Waker,
}

pub(crate) struct HostState {
    pub exts: Vec<Runtime>,
    pub popups: VecDeque<PendingPopup>,
    /// Things that went wrong loading extensions, for the Extensions page.
    pub problems: Vec<String>,
}

pub(crate) struct Runtime {
    pub slot: ExtSlot,
    /// Uninstalled during this run: its slot is never reused, and the UI no longer sees it.
    pub removed: bool,
    pub manifest: Manifest,
    pub source: Source,
    pub on: bool,
    pub status: Status,
    pub note: Option<String>,
    pub view: Arc<Vec<Widget>>,
    pub badge: u32,
    pub badge_summary: String,
    pub badge_meter: Option<BadgeMeter>,
    pub control: Option<Sender<Event>>,
    pub thread: Option<JoinHandle<()>>,
}

impl Inner {
    pub fn lock(&self) -> Option<MutexGuard<'_, HostState>> {
        self.state.lock().ok()
    }

    /// Runs `f` on the extension in `slot`, then wakes the UI.
    pub fn update(&self, slot: ExtSlot, f: impl FnOnce(&mut Runtime)) {
        if let Some(mut state) = self.lock()
            && let Some(rt) = state.exts.iter_mut().find(|r| r.slot == slot)
        {
            f(rt);
        }
        self.waker.poke();
    }

    pub fn push_popup(&self, popup: PendingPopup) {
        if let Some(mut state) = self.lock() {
            if state.popups.len() >= MAX_QUEUED_POPUPS {
                state.popups.pop_front();
            }
            state.popups.push_back(popup);
        }
        self.waker.poke();
    }

    fn data_dir(&self, id: &str) -> PathBuf {
        self.root.data_dir(id)
    }

    fn helper(&self, manifest: &Manifest) -> PathBuf {
        self.root.helper(&manifest.id, &manifest.helper)
    }
}

/// The installed name, if the dev package in `src` has the id of an extension installed from the store: a dev
/// build never replaces a signed one.
fn from_store(reg: &Registry, src: &Path) -> Option<String> {
    let text = std::fs::read_to_string(src.join("extension.toml")).ok()?;
    let manifest = Manifest::parse(&text).ok()?;
    reg.get(&manifest.id)
        .filter(|i| i.source == Source::Catalog)
        .map(|_| manifest.name)
}

/// Samo's handle on its extensions. Cheap to clone.
#[derive(Clone)]
pub struct Host {
    pub(crate) inner: Arc<Inner>,
}

impl Host {
    /// Loads what is installed (installing dev extensions first, if `dev_dir` is set) and starts every helper
    /// that is on. `wake` is called, coalesced, whenever there is news for the UI.
    pub fn start(root: ExtRoot, config: HostConfig, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let waker = Waker::start(wake, config.wake_every);
        let mut problems = Vec::new();
        let mut registry = match Registry::load(&root) {
            Ok(r) => Some(r),
            Err(e) => {
                problems.push(e.to_string());
                None
            }
        };

        if let (Some(dev_dir), Some(reg)) = (&config.dev_dir, registry.as_mut()) {
            for src in find_dev_packages(Path::new(dev_dir)) {
                if let Some(name) = from_store(reg, &src) {
                    problems.push(format!(
                        "The dev build in {} wasn't used: {name} is installed from the store. Uninstall it to go \
                         back to the dev build",
                        src.display()
                    ));
                    continue;
                }
                match install_dev(&root, &src) {
                    Ok(done) => {
                        let id = done.manifest.id.clone();
                        let before = reg.get(&id).cloned();
                        reg.upsert(Installed {
                            id: id.clone(),
                            version: done.manifest.version.clone(),
                            on: before.as_ref().is_none_or(|b| b.on),
                            source: Source::Dev {
                                path: src.to_string_lossy().into_owned(),
                            },
                        });
                        if done.copied {
                            let event = if before.is_some() {
                                "update"
                            } else {
                                "install"
                            };
                            History::append(
                                &root,
                                event,
                                &id,
                                &format!("dev {} {}", done.manifest.version, src.display()),
                            );
                        }
                    }
                    Err(e) => problems.push(format!("{}: {e}", src.display())),
                }
            }
            if let Err(e) = reg.save(&root) {
                problems.push(e.to_string());
            }
        }

        let mut exts = Vec::new();
        for installed in registry
            .map(|r| r.extensions)
            .unwrap_or_default()
            .into_iter()
            .take(MAX_EXTENSIONS)
        {
            let manifest_path = root.current(&installed.id).join("extension.toml");
            let manifest = std::fs::read_to_string(&manifest_path)
                .map_err(|e| e.to_string())
                .and_then(|text| Manifest::parse(&text).map_err(|e| e.to_string()));
            let manifest = match manifest {
                Ok(m) => m,
                Err(e) => {
                    problems.push(format!("{}: {e}", installed.id));
                    continue;
                }
            };
            let slot = ExtSlot(u8::try_from(exts.len()).unwrap_or(u8::MAX));
            exts.push(Runtime {
                slot,
                removed: false,
                manifest,
                source: installed.source,
                on: installed.on,
                status: if installed.on {
                    Status::Starting
                } else {
                    Status::Off
                },
                note: None,
                view: Arc::new(Vec::new()),
                badge: 0,
                badge_summary: String::new(),
                badge_meter: None,
                control: None,
                thread: None,
            });
        }

        let to_launch: Vec<ExtSlot> = exts.iter().filter(|r| r.on).map(|r| r.slot).collect();
        let host = Self {
            inner: Arc::new(Inner {
                root,
                config,
                state: Mutex::new(HostState {
                    exts,
                    popups: VecDeque::new(),
                    problems,
                }),
                waker,
            }),
        };
        for slot in to_launch {
            host.launch(slot);
        }
        host
    }

    /// Starts the helper in `slot`, unless it can't run here.
    pub(crate) fn launch(&self, slot: ExtSlot) {
        let config = &self.inner.config;
        let Some(mut state) = self.inner.lock() else {
            return;
        };
        let Some(rt) = state.exts.iter_mut().find(|r| r.slot == slot) else {
            return;
        };
        if rt.control.is_some() {
            return;
        }
        if !may_run(&rt.source, config.dev_dir.as_deref()) {
            rt.status = Status::Blocked(
                "Dev build: start Samo with SAMO_EXT_DEV_DIR pointing at it to run it".into(),
            );
            drop(state);
            self.inner.waker.poke();
            return;
        }
        if !samo_satisfies(&rt.manifest.min_samo, &config.samo_version) {
            rt.status = Status::Blocked(format!("Needs Samo {} or newer", rt.manifest.min_samo));
            drop(state);
            self.inner.waker.poke();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let job = supervisor::Job {
            inner: Arc::clone(&self.inner),
            slot,
            id: rt.manifest.id.clone(),
            helper: self.inner.helper(&rt.manifest),
            data_dir: self.inner.data_dir(&rt.manifest.id),
            events: tx.clone(),
        };
        rt.status = Status::Starting;
        rt.note = None;
        rt.control = Some(tx);
        let spawned = thread::Builder::new()
            .name(format!("samo-ext-{}", rt.manifest.id))
            .spawn(move || supervisor::run(job, rx));
        match spawned {
            Ok(handle) => rt.thread = Some(handle),
            Err(e) => {
                rt.control = None;
                rt.status = Status::Stopped(format!("Samo couldn't start a thread for it: {e}"));
            }
        }
        drop(state);
        self.inner.waker.poke();
    }

    /// Stops the helper in `slot` and waits for it to be gone.
    pub(crate) fn stop(&self, slot: ExtSlot) {
        let taken = self.inner.lock().and_then(|mut state| {
            state
                .exts
                .iter_mut()
                .find(|r| r.slot == slot)
                .map(|rt| (rt.control.take(), rt.thread.take()))
        });
        if let Some((control, thread)) = taken {
            if let Some(control) = control {
                let _ = control.send(Event::Stop);
            }
            if let Some(thread) = thread {
                let _ = thread.join();
            }
        }
    }

    pub fn snapshot(&self) -> Arc<ExtSnapshot> {
        let Some(state) = self.inner.lock() else {
            return Arc::new(ExtSnapshot::default());
        };
        Arc::new(ExtSnapshot {
            extensions: state
                .exts
                .iter()
                .filter(|rt| !rt.removed)
                .map(|rt| ExtView {
                    slot: rt.slot,
                    id: rt.manifest.id.clone(),
                    name: rt.manifest.name.clone(),
                    summary: rt.manifest.summary.clone(),
                    version: rt.manifest.version.clone(),
                    dev: matches!(rt.source, Source::Dev { .. }),
                    on: rt.on,
                    status: rt.status.clone(),
                    note: rt.note.clone(),
                    changes: rt.manifest.changes.clone(),
                    view: Arc::clone(&rt.view),
                    badge: if rt.status == Status::Running {
                        rt.badge
                    } else {
                        0
                    },
                    badge_summary: rt.badge_summary.clone(),
                    badge_meter: if rt.status == Status::Running {
                        rt.badge_meter.clone()
                    } else {
                        None
                    },
                })
                .collect(),
            problems: state.problems.clone(),
        })
    }

    /// The pop-ups waiting to be shown, oldest first; each is returned once.
    pub fn take_popups(&self) -> Vec<PendingPopup> {
        self.inner
            .lock()
            .map(|mut state| state.popups.drain(..).collect())
            .unwrap_or_default()
    }

    fn send(&self, slot: ExtSlot, msg: HostMsg) {
        if let Some(state) = self.inner.lock()
            && let Some(control) = state
                .exts
                .iter()
                .find(|r| r.slot == slot)
                .and_then(|r| r.control.as_ref())
        {
            let _ = control.send(Event::Send(msg));
        }
    }

    /// The user flipped one of the extension's switches.
    pub fn setting(&self, slot: ExtSlot, key: &str, value: bool) {
        self.send(
            slot,
            HostMsg::Setting {
                key: key.to_string(),
                value,
            },
        );
    }

    /// The user pressed one of the extension's buttons.
    pub fn action(&self, slot: ExtSlot, id: &str) {
        self.send(slot, HostMsg::Action { id: id.to_string() });
    }

    /// Whether the extension's page is on screen.
    pub fn visible(&self, slot: ExtSlot, page: bool) {
        self.send(slot, HostMsg::Visible { page });
    }

    /// Turns an extension on or off. Off stops its helper and runs `<helper> off`, which undoes what is active;
    /// on runs `<helper> on`, then starts it. Runs in the background; the UI sees the result through `snapshot`.
    pub fn set_on(&self, slot: ExtSlot, on: bool) {
        let Some(manifest) = self.inner.lock().and_then(|mut state| {
            let rt = state.exts.iter_mut().find(|r| r.slot == slot)?;
            if rt.on == on {
                return None;
            }
            rt.on = on;
            rt.status = if on { Status::Starting } else { Status::Off };
            Some(rt.manifest.clone())
        }) else {
            return;
        };
        self.inner.waker.poke();
        let host = self.clone();
        let _ = thread::Builder::new()
            .name(format!("samo-ext-{}-switch", manifest.id))
            .spawn(move || host.switch(slot, &manifest, on));
    }

    fn switch(&self, slot: ExtSlot, manifest: &Manifest, on: bool) {
        let inner = &self.inner;
        let root = &inner.root;
        let id = &manifest.id;
        if !on {
            self.stop(slot);
        }
        let verb = if on { "on" } else { "off" };
        let may = self.inner.lock().and_then(|state| {
            state
                .exts
                .iter()
                .find(|r| r.slot == slot)
                .map(|rt| may_run(&rt.source, inner.config.dev_dir.as_deref()))
        });
        let result = if may == Some(true) {
            one_shot(
                &inner.helper(manifest),
                verb,
                &inner.data_dir(id),
                &inner.config,
            )
        } else {
            Ok(())
        };
        if let Ok(mut reg) = Registry::load(root) {
            reg.set_on(id, on);
            let _ = reg.save(root);
        }
        History::append(root, verb, id, &result.clone().err().unwrap_or_default());
        inner.update(slot, |rt| {
            rt.badge = 0;
            rt.note = result.err();
        });
        if on {
            self.launch(slot);
        }
    }

    /// Starts a stopped extension again, forgetting its past crashes.
    pub fn restart(&self, slot: ExtSlot) {
        self.stop(slot);
        let on = self
            .inner
            .lock()
            .and_then(|state| state.exts.iter().find(|r| r.slot == slot).map(|r| r.on));
        if on == Some(true) {
            self.launch(slot);
        }
    }

    /// Stops every helper (each gets `shutdown`, then is killed after the grace period). Call before quitting.
    pub fn shutdown_all(&self) {
        let taken: Vec<_> = self
            .inner
            .lock()
            .map(|mut state| {
                state
                    .exts
                    .iter_mut()
                    .map(|rt| (rt.control.take(), rt.thread.take()))
                    .collect()
            })
            .unwrap_or_default();
        for (control, _) in &taken {
            if let Some(control) = control {
                let _ = control.send(Event::Stop);
            }
        }
        for (_, thread) in taken {
            if let Some(thread) = thread {
                let _ = thread.join();
            }
        }
        self.inner.waker.close();
    }

    pub fn root(&self) -> &ExtRoot {
        &self.inner.root
    }
}
