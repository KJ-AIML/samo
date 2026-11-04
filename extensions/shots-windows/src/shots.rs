//! Shots kept in memory only while a window shows them (R17), and none lost when Shots stops (R19): whatever is still
//! open and was never saved is saved as `… (unsaved).png`, and so are edits not yet copied or saved, beside the shot
//! and never over it. Ported from the Mac's `ShotStore.swift`.

use crate::after::{Disk, Shot};
use crate::encoding;
use crate::naming;
use crate::settings::{Format, Settings};
use image::RgbaImage;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A shot in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShotId(u64);

/// An editor's export while it has changes not yet copied or saved.
pub type Edits = Box<dyn Fn() -> Option<RgbaImage>>;

struct Entry {
    shot: Shot,
    holders: u32,
    edits: Option<Edits>,
}

#[derive(Default)]
pub struct ShotStore {
    next: u64,
    entries: BTreeMap<ShotId, Entry>,
}

impl ShotStore {
    /// A window shows a new shot: it is kept from now until the last window showing it closes. There is no shot
    /// held by nothing, so none is kept or saved at shutdown without a window (R17, R19).
    pub fn show(&mut self, shot: Shot) -> ShotId {
        self.next += 1;
        let id = ShotId(self.next);
        self.entries.insert(
            id,
            Entry {
                shot,
                holders: 1,
                edits: None,
            },
        );
        id
    }

    pub fn get(&self, id: ShotId) -> Option<&Shot> {
        self.entries.get(&id).map(|e| &e.shot)
    }

    pub fn get_mut(&mut self, id: ShotId) -> Option<&mut Shot> {
        self.entries.get_mut(&id).map(|e| &mut e.shot)
    }

    /// Another window now shows it.
    pub fn retain(&mut self, id: ShotId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.holders += 1;
        }
    }

    /// An editor showing it hands over how to get its unsaved edits, or `None` when it has none.
    pub fn set_edits(&mut self, id: ShotId, edits: Option<Edits>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.edits = edits;
        }
    }

    /// A window showing it closed; the last one lets it go.
    pub fn release(&mut self, id: ShotId) {
        let gone = self.entries.get_mut(&id).is_some_and(|entry| {
            entry.holders = entry.holders.saturating_sub(1);
            entry.holders == 0
        });
        if gone {
            self.entries.remove(&id);
        }
    }

    pub fn live(&self) -> usize {
        self.entries.len()
    }

    /// Saves, as `<name> (unsaved).png`, every shown shot never saved (into the folder; `home` is `~`) and every
    /// editor's unsaved edits (beside the shot's file, never over it). Returns where.
    pub fn save_unsaved(
        &mut self,
        settings: &Settings,
        home: &Path,
        disk: &mut impl Disk,
    ) -> Vec<PathBuf> {
        let mut saved = Vec::new();
        for entry in self.entries.values_mut() {
            let shot = &mut entry.shot;
            let edited = entry.edits.as_ref().and_then(|edits| edits());
            let image = match (&edited, &shot.saved_to) {
                (Some(edited), _) => edited,
                (None, None) => &shot.image,
                (None, Some(_)) => continue,
            };
            let Some(data) = encoding::data(image, Format::Png, shot.scale) else {
                continue;
            };
            let folder = shot
                .saved_to
                .as_ref()
                .and_then(|p| p.parent())
                .map_or_else(|| settings.folder_path(home), Path::to_path_buf);
            let stem = match &shot.saved_to {
                Some(path) => path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                None => naming::name(shot.taken_at, Format::Png)
                    .trim_end_matches(".png")
                    .to_string(),
            };
            if disk.make_folder(&folder).is_err() {
                continue;
            }
            let path = naming::unique(&format!("{stem} (unsaved).png"), &folder, |p| {
                disk.exists(p)
            });
            if disk.write(&data, &path).is_ok() {
                if shot.saved_to.is_none() {
                    shot.saved_to = Some(path.clone());
                }
                saved.push(path);
            }
        }
        saved
    }
}

pub struct PinMath;

impl PinMath {
    /// A pin's opacity after a scroll: between 20 % and 100 %.
    pub fn opacity(current: f64, scrolled_by: f64) -> f64 {
        (current + scrolled_by * 0.02).clamp(0.2, 1.0)
    }

    /// A pin's size, in the target monitor's pixels at 100 % (device-independent): the shot's pixels over its scale,
    /// shrunk to fit 80 % of `screen` (in the same units). Multiply by the monitor's scale for its physical pixels.
    pub fn size(pixels: (f64, f64), scale: f64, screen: (f64, f64)) -> (f64, f64) {
        let scale = scale.max(1.0);
        let (w, h) = (pixels.0 / scale, pixels.1 / scale);
        let fit = 1f64
            .min(screen.0 * 0.8 / w.max(1.0))
            .min(screen.1 * 0.8 / h.max(1.0));
        ((w * fit).round(), (h * fit).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::after::tests::{FakeDisk, Log, plain};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn disk() -> FakeDisk {
        FakeDisk {
            log: Rc::new(RefCell::new(Log::default())),
            files: HashMap::new(),
            fails: false,
        }
    }

    #[test]
    fn a_shot_is_let_go_when_its_windows_close() {
        let mut store = ShotStore::default();
        // A pin shows it, and an editor too.
        let id = store.show(Shot::new(plain(8, 8), 2.0, chrono::Local::now()));
        store.retain(id);
        store.release(id);
        assert!(store.get(id).is_some(), "one window still shows it");
        assert_eq!(store.live(), 1);
        store.release(id);
        assert!(
            store.get(id).is_none(),
            "the last window closed, so the image is freed"
        );
        assert_eq!(store.live(), 0);
    }

    #[test]
    fn shutdown_saves_what_was_never_saved() {
        let mut store = ShotStore::default();
        let taken = chrono::DateTime::from_timestamp(1_793_543_712, 0)
            .map(|t| t.with_timezone(&chrono::Local))
            .unwrap_or_default();
        let pinned_only = store.show(Shot::new(plain(8, 8), 2.0, taken));
        let mut saved = Shot::new(plain(8, 8), 2.0, chrono::Local::now());
        saved.saved_to = Some(PathBuf::from("/tmp/already.png"));
        let saved = store.show(saved);
        let mut disk = disk();
        let written = store.save_unsaved(&Settings::default(), Path::new("/h"), &mut disk);
        assert_eq!(written.len(), 1, "only the shot never saved");
        let name = written
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(name.ends_with(" (unsaved).png"), "{name}");
        assert_eq!(
            store.get(pinned_only).and_then(|s| s.saved_to.clone()),
            written.first().cloned()
        );
        assert!(written.first().is_some_and(|p| disk.files.contains_key(p)));
        // Saving again writes nothing more.
        assert!(
            store
                .save_unsaved(&Settings::default(), Path::new("/h"), &mut disk)
                .is_empty()
        );
    }

    #[test]
    fn edits_are_saved_beside_the_shot_never_over_it() {
        let mut store = ShotStore::default();
        let mut shot = Shot::new(plain(8, 8), 1.0, chrono::Local::now());
        shot.saved_to = Some(PathBuf::from("/shots/Shot 1.png"));
        let id = store.show(shot);
        store.set_edits(id, Some(Box::new(|| Some(plain(4, 4)))));
        let mut disk = disk();
        let written = store.save_unsaved(&Settings::default(), Path::new("/h"), &mut disk);
        assert_eq!(written, [PathBuf::from("/shots/Shot 1 (unsaved).png")]);
    }

    #[test]
    fn a_pins_opacity_and_size() {
        assert!((PinMath::opacity(1.0, 10.0) - 1.0).abs() < 1e-9);
        assert!((PinMath::opacity(1.0, -10.0) - 0.8).abs() < 1e-9);
        assert!((PinMath::opacity(0.3, -100.0) - 0.2).abs() < 1e-9);
        // A shot at twice the pixels pins at its real size, shrunk to fit the screen.
        assert_eq!(
            PinMath::size((800.0, 400.0), 2.0, (1440.0, 900.0)),
            (400.0, 200.0)
        );
        assert_eq!(
            PinMath::size((5120.0, 2880.0), 2.0, (1440.0, 900.0)),
            (1152.0, 648.0)
        );
    }
}
