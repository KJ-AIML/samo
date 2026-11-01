//! Installing, updating and uninstalling from the store (ADR-003 M3). These run on the store's worker thread and
//! may take seconds: they stop and start helpers and wait for them.

use crate::host::{Host, Runtime};
use crate::install::{Placed, place, point_current, remove_other_versions, stage_catalog};
use crate::state::{ExtSlot, Status};
use crate::supervisor::one_shot;
use crate::{History, InstallError, Installed, MAX_EXTENSIONS, Registry};
use samo_domain::extensions::{Source, may_run, samo_satisfies};
use samo_domain::store::Listing;
use samo_ext_protocol::{Manifest, PROTOCOL_VERSION};
use std::fmt;
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// What an install from the store did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreInstall {
    Installed {
        version: String,
    },
    Updated {
        from: String,
        to: String,
    },
    /// A dev build of the same extension was replaced by the signed one; its data and switches are kept.
    ReplacedDev {
        version: String,
    },
}

/// Why an uninstall stopped before deleting anything (R7). **Uninstall anyway** deletes regardless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UninstallFailed {
    NotInstalled,
    /// A dev build without `SAMO_EXT_DEV_DIR` can't run, so it can't undo what it changed.
    CantRun,
    /// `off` or `uninstall` failed; `said` is the error with what the helper printed.
    Step {
        step: &'static str,
        said: String,
    },
    /// Its folder couldn't be deleted.
    Delete(String),
}

impl fmt::Display for UninstallFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => write!(f, "it isn't installed"),
            Self::CantRun => write!(
                f,
                "this dev build can't run here, so it can't undo what it changed. Start Samo with \
                 SAMO_EXT_DEV_DIR pointing at its folder, or uninstall it anyway"
            ),
            Self::Step { step, said } => write!(f, "`{step}` didn't finish ({said})"),
            Self::Delete(e) => write!(f, "its folder couldn't be deleted ({e})"),
        }
    }
}

/// What was there before an install, to put back if the new version doesn't start.
struct Before {
    slot: ExtSlot,
    manifest: Manifest,
    source: Source,
    on: bool,
}

impl Host {
    fn find(&self, id: &str) -> Option<Before> {
        let state = self.inner.lock()?;
        let rt = state
            .exts
            .iter()
            .find(|r| !r.removed && r.manifest.id == id)?;
        Some(Before {
            slot: rt.slot,
            manifest: rt.manifest.clone(),
            source: rt.source.clone(),
            on: rt.on,
        })
    }

    fn record(
        &self,
        id: &str,
        version: &str,
        on: bool,
        source: Source,
    ) -> Result<(), InstallError> {
        let root = &self.inner.root;
        let mut reg = Registry::load(root)?;
        reg.upsert(Installed {
            id: id.into(),
            version: version.into(),
            on,
            source,
        });
        reg.save(root)
    }

    /// Waits for the helper in `slot` to say hello. `false` if it stops, can't run, or stays silent.
    fn comes_up(&self, slot: ExtSlot) -> bool {
        let deadline = Instant::now() + self.inner.config.hello_timeout + Duration::from_secs(3);
        while Instant::now() < deadline {
            let status = self.inner.lock().and_then(|s| {
                s.exts
                    .iter()
                    .find(|r| r.slot == slot)
                    .map(|r| r.status.clone())
            });
            match status {
                Some(Status::Running) => return true,
                Some(Status::Stopped(_) | Status::Blocked(_)) | None => return false,
                _ => thread::sleep(Duration::from_millis(20)),
            }
        }
        false
    }

    /// Installs `listing` from its package, whose hash already matched the signed catalog. A new extension starts;
    /// an installed one is updated, or its dev build replaced, without running `off` or `uninstall` and keeping
    /// `data/` and its on/off switch. If the new version's helper doesn't start, the old one is put back.
    pub fn install_from_store(
        &self,
        listing: &Listing,
        package: &[u8],
    ) -> Result<StoreInstall, InstallError> {
        let inner = &self.inner;
        let root = &inner.root;
        let id = &listing.id;
        if listing.protocol != PROTOCOL_VERSION
            || !samo_satisfies(&listing.min_samo, &inner.config.samo_version)
        {
            return Err(InstallError::Refused(format!(
                "{} needs Samo {} or newer",
                listing.name, listing.min_samo
            )));
        }
        let before = self.find(id);
        if before.is_none() {
            let (live, slots) = inner
                .lock()
                .map(|s| (s.exts.iter().filter(|r| !r.removed).count(), s.exts.len()))
                .unwrap_or((MAX_EXTENSIONS, 0));
            if live >= MAX_EXTENSIONS || u8::try_from(slots).is_err() {
                return Err(InstallError::Refused(format!(
                    "Samo runs at most {MAX_EXTENSIONS} extensions"
                )));
            }
        }

        // Everything is checked in staging before the running extension is touched.
        let staged = stage_catalog(root, listing, package)?;
        let manifest = staged.manifest.clone();
        if let Some(b) = &before {
            self.stop(b.slot);
        }
        let relaunch_old = |b: &Option<Before>| {
            if let Some(b) = b
                && b.on
            {
                self.launch(b.slot);
            }
        };
        let placed = match place(root, staged) {
            Ok(p) => p,
            Err(e) => {
                relaunch_old(&before);
                return Err(e);
            }
        };
        let on = before.as_ref().is_none_or(|b| b.on);
        let switched = point_current(root, id, &manifest.version)
            .and_then(|()| self.record(id, &manifest.version, on, Source::Catalog));
        if let Err(e) = switched {
            self.put_back(&before, placed);
            relaunch_old(&before);
            return Err(e);
        }
        let _ = fs::create_dir_all(root.data_dir(id));

        let slot = match &before {
            Some(b) => {
                self.inner.update(b.slot, |rt| {
                    rt.manifest = manifest.clone();
                    rt.source = Source::Catalog;
                    rt.status = if on { Status::Starting } else { Status::Off };
                    rt.note = None;
                });
                b.slot
            }
            None => self.add_runtime(manifest.clone()),
        };
        if on {
            self.launch(slot);
            if let Some(b) = &before
                && !self.comes_up(slot)
            {
                self.stop(slot);
                self.put_back(&before, placed);
                self.inner.update(slot, |rt| {
                    rt.manifest = b.manifest.clone();
                    rt.source = b.source.clone();
                    rt.status = Status::Starting;
                });
                self.launch(slot);
                let why = format!(
                    "{} {} didn't start, so Samo went back to {}",
                    manifest.name, manifest.version, b.manifest.version
                );
                History::append(root, "update-failed", id, &why);
                return Err(InstallError::Refused(why));
            }
        }
        placed.keep();
        remove_other_versions(&root.ext_dir(id), &manifest.version);

        let (event, outcome) = match before {
            None => (
                "install",
                StoreInstall::Installed {
                    version: manifest.version.clone(),
                },
            ),
            Some(b) if matches!(b.source, Source::Dev { .. }) => (
                "replace-dev",
                StoreInstall::ReplacedDev {
                    version: manifest.version.clone(),
                },
            ),
            Some(b) => (
                "update",
                StoreInstall::Updated {
                    from: b.manifest.version,
                    to: manifest.version.clone(),
                },
            ),
        };
        History::append(root, event, id, &format!("catalog {}", manifest.version));
        Ok(outcome)
    }

    /// Undoes a placed install: the old version folder, `current` and the registry entry come back.
    fn put_back(&self, before: &Option<Before>, placed: Placed) {
        let root = &self.inner.root;
        placed.undo();
        if let Some(b) = before {
            let _ = point_current(root, &b.manifest.id, &b.manifest.version);
            let _ = self.record(&b.manifest.id, &b.manifest.version, b.on, b.source.clone());
        }
    }

    fn add_runtime(&self, manifest: Manifest) -> ExtSlot {
        let mut slot = ExtSlot(u8::MAX);
        if let Some(mut state) = self.inner.lock() {
            slot = ExtSlot(u8::try_from(state.exts.len()).unwrap_or(u8::MAX));
            state.exts.push(Runtime {
                slot,
                removed: false,
                manifest,
                source: Source::Catalog,
                on: true,
                status: Status::Starting,
                note: None,
                view: Arc::new(Vec::new()),
                badge: 0,
                badge_summary: String::new(),
                badge_meter: None,
                control: None,
                thread: None,
            });
        }
        self.inner.waker.poke();
        slot
    }

    /// Uninstalls `id`: turns it off (running `off`), runs `uninstall`, then deletes its folder, `data/` included,
    /// and its registry entry. If a step fails, or a dev build can't run here, nothing is deleted unless `anyway`.
    pub fn uninstall(&self, id: &str, anyway: bool) -> Result<(), UninstallFailed> {
        let inner = &self.inner;
        let root = &inner.root;
        let b = self.find(id).ok_or(UninstallFailed::NotInstalled)?;
        self.stop(b.slot);
        let helper = root.helper(id, &b.manifest.helper);
        let data = root.data_dir(id);
        let failed = if !may_run(&b.source, inner.config.dev_dir.as_deref()) {
            Some(UninstallFailed::CantRun)
        } else {
            let off = if b.on {
                one_shot(&helper, "off", &data, &inner.config)
            } else {
                Ok(())
            };
            off.map_err(|said| UninstallFailed::Step { step: "off", said })
                .and_then(|()| {
                    one_shot(&helper, "uninstall", &data, &inner.config).map_err(|said| {
                        UninstallFailed::Step {
                            step: "uninstall",
                            said,
                        }
                    })
                })
                .err()
        };
        if let Some(why) = &failed
            && !anyway
        {
            // Its helper is stopped and `off` may have run: it stays installed, and off.
            if let Ok(mut reg) = Registry::load(root) {
                reg.set_on(id, false);
                let _ = reg.save(root);
            }
            inner.update(b.slot, |rt| {
                rt.on = false;
                rt.status = Status::Off;
                rt.note = Some(format!("Uninstall stopped: {why}"));
            });
            History::append(root, "uninstall-failed", id, &why.to_string());
            return Err(why.clone());
        }
        fs::remove_dir_all(root.ext_dir(id)).map_err(|e| UninstallFailed::Delete(e.to_string()))?;
        if let Ok(mut reg) = Registry::load(root) {
            reg.remove(id);
            let _ = reg.save(root);
        }
        inner.update(b.slot, |rt| {
            rt.removed = true;
            rt.on = false;
            rt.status = Status::Off;
        });
        let detail = match failed {
            Some(why) => format!("anyway, after: {why}"),
            None => String::new(),
        };
        History::append(root, "uninstall", id, &detail);
        Ok(())
    }
}
