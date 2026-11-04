//! After a capture (R19: before any window opens): the shot is copied, saved, or both, then opened in the editor,
//! pinned, or nothing. Ported from the Mac's `AfterCapture.swift`.

pub use crate::clipboard::Clipboard;
use crate::encoding;
use crate::geom::Point;
use crate::naming;
use crate::settings::{After, Format, Settings, Then};
use chrono::{DateTime, Local};
use image::RgbaImage;
use std::path::{Path, PathBuf};

/// The disk; tests stand in for it.
pub trait Disk {
    fn make_folder(&mut self, dir: &Path) -> Result<(), String>;
    fn exists(&self, path: &Path) -> bool;
    /// Writes a new file; never over one.
    fn write(&mut self, data: &[u8], to: &Path) -> Result<(), String>;
    /// Writes over `at`, all at once (Save in the editor, over the shot's own file).
    fn replace(&mut self, data: &[u8], at: &Path) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutcome {
    pub saved_to: Option<PathBuf>,
    pub bytes: usize,
    pub copied: bool,
    /// What opened after: the editor, a pin, or nothing.
    pub then: Then,
    pub problems: Vec<String>,
}

impl CaptureOutcome {
    /// The note shown near the pointer.
    pub fn note(&self) -> String {
        if let Some(problem) = self.problems.first() {
            return problem.clone();
        }
        match (self.copied, &self.saved_to) {
            (true, Some(_)) => "Copied and saved".into(),
            (true, None) => "Copied".into(),
            (false, Some(path)) => {
                let folder = path
                    .parent()
                    .and_then(Path::file_name)
                    .map_or_else(|| "Shots".to_string(), |n| n.to_string_lossy().into_owned());
                format!("Saved to {folder}")
            }
            (false, None) => "Captured".into(),
        }
    }
}

/// A shot, kept only while something shows or needs it.
#[derive(Debug, Clone)]
pub struct Shot {
    pub image: RgbaImage,
    /// Its pixels per Windows pixel at 100 % (the monitor's scale), for showing it at its real size.
    pub scale: f64,
    pub taken_at: DateTime<Local>,
    pub saved_to: Option<PathBuf>,
    /// Where it was on screen, so a pin opens where the shot was taken.
    pub origin: Option<Point>,
}

impl Shot {
    pub fn new(image: RgbaImage, scale: f64, taken_at: DateTime<Local>) -> Self {
        Self {
            image,
            scale,
            taken_at,
            saved_to: None,
            origin: None,
        }
    }
}

/// Copies and/or saves `shot` as `settings` say (`home` is `~`), then calls `open` with what comes next.
pub fn run(
    shot: &mut Shot,
    settings: &Settings,
    home: &Path,
    clipboard: &mut impl Clipboard,
    disk: &mut impl Disk,
    open: impl FnOnce(&Shot, Then),
) -> CaptureOutcome {
    let mut outcome = CaptureOutcome {
        saved_to: None,
        bytes: 0,
        copied: false,
        then: settings.then,
        problems: Vec::new(),
    };
    let one_to_one;
    let (image, density) = if settings.retina || shot.scale <= 1.0 {
        (&shot.image, shot.scale)
    } else {
        one_to_one = encoding::scaled(&shot.image, 1.0 / shot.scale);
        (&one_to_one, 1.0)
    };
    // Encoded once when the clipboard and the file both want PNG.
    let mut png = None;
    if matches!(settings.after, After::Copy | After::Both) {
        png = encoding::data(image, Format::Png, density);
        if png.as_deref().is_some_and(|bytes| clipboard.put_png(bytes)) {
            outcome.copied = true;
        } else {
            outcome.problems.push("Couldn't copy the shot".into());
        }
    }
    if matches!(settings.after, After::Save | After::Both) {
        let folder = settings.folder_path(home);
        let saved = (|| -> Result<(PathBuf, usize), String> {
            let reused = (settings.format == Format::Png)
                .then(|| png.take())
                .flatten();
            let data = reused
                .or_else(|| encoding::data(image, settings.format, density))
                .ok_or("it couldn't be encoded")?;
            disk.make_folder(&folder)?;
            let name = naming::name(shot.taken_at, settings.format);
            let path = naming::unique(&name, &folder, |p| disk.exists(p));
            disk.write(&data, &path)?;
            Ok((path, data.len()))
        })();
        match saved {
            Ok((path, bytes)) => {
                outcome.bytes = bytes;
                shot.saved_to = Some(path.clone());
                outcome.saved_to = Some(path);
            }
            Err(e) => outcome.problems.push(format!(
                "Couldn't save the shot to {}: {e}",
                folder.display()
            )),
        }
    }
    if settings.then != Then::Nothing {
        open(shot, settings.then);
    }
    outcome
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    pub(crate) fn plain(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba([51, 77, 179, 255]))
    }

    /// What happened, in order.
    #[derive(Default)]
    pub(crate) struct Log {
        pub events: Vec<String>,
        pub copied: Option<Vec<u8>>,
    }

    pub(crate) struct FakeClipboard {
        pub log: Rc<RefCell<Log>>,
        pub works: bool,
    }

    impl Clipboard for FakeClipboard {
        fn put_png(&mut self, png: &[u8]) -> bool {
            let mut log = self.log.borrow_mut();
            log.events.push("copy".into());
            log.copied = Some(png.to_vec());
            self.works
        }
        fn put_text(&mut self, _text: &str) -> bool {
            self.log.borrow_mut().events.push("copy text".into());
            self.works
        }
    }

    pub(crate) struct FakeDisk {
        pub log: Rc<RefCell<Log>>,
        pub files: HashMap<PathBuf, Vec<u8>>,
        pub fails: bool,
    }

    impl Disk for FakeDisk {
        fn make_folder(&mut self, _dir: &Path) -> Result<(), String> {
            Ok(())
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }
        fn write(&mut self, data: &[u8], to: &Path) -> Result<(), String> {
            self.log.borrow_mut().events.push("save".into());
            if self.fails {
                return Err("Access is denied.".into());
            }
            self.files.insert(to.to_path_buf(), data.to_vec());
            Ok(())
        }
        fn replace(&mut self, data: &[u8], at: &Path) -> Result<(), String> {
            self.log.borrow_mut().events.push("replace".into());
            if self.fails {
                return Err("Access is denied.".into());
            }
            self.files.insert(at.to_path_buf(), data.to_vec());
            Ok(())
        }
    }

    fn world() -> (Rc<RefCell<Log>>, FakeClipboard, FakeDisk) {
        let log = Rc::new(RefCell::new(Log::default()));
        let clipboard = FakeClipboard {
            log: Rc::clone(&log),
            works: true,
        };
        let disk = FakeDisk {
            log: Rc::clone(&log),
            files: HashMap::new(),
            fails: false,
        };
        (log, clipboard, disk)
    }

    fn shot(w: u32, h: u32, scale: f64) -> Shot {
        Shot::new(plain(w, h), scale, chrono::Local::now())
    }

    const HOME: &str = "/h";

    #[test]
    fn the_capture_action_runs_before_windows() {
        let (log, mut clipboard, mut disk) = world();
        let mut s = shot(40, 20, 2.0);
        let mut settings = Settings::default();
        let opened = Rc::clone(&log);
        let outcome = run(
            &mut s,
            &settings,
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, then| {
                opened.borrow_mut().events.push(then.key().into());
            },
        );
        // Copied and saved first; only then does the editor open (the default).
        assert_eq!(log.borrow().events, ["copy", "save", "edit"]);
        assert_eq!(outcome.note(), "Copied and saved");
        assert_eq!(s.saved_to, outcome.saved_to);
        assert!(outcome.bytes > 0);
        // Pin: the pin opens last.
        log.borrow_mut().events.clear();
        settings.after = After::Copy;
        settings.then = Then::Pin;
        let opened = Rc::clone(&log);
        let pinned = run(
            &mut s,
            &settings,
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, then| {
                opened.borrow_mut().events.push(then.key().into());
            },
        );
        assert_eq!(log.borrow().events, ["copy", "pin"]);
        assert_eq!(pinned.note(), "Copied");
        // Nothing: nothing opens.
        log.borrow_mut().events.clear();
        settings.then = Then::Nothing;
        let opened = Rc::clone(&log);
        let _ = run(
            &mut s,
            &settings,
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, then| {
                opened.borrow_mut().events.push(then.key().into());
            },
        );
        assert_eq!(log.borrow().events, ["copy"]);
    }

    #[test]
    fn the_editor_opens_after_copy_and_save() {
        let (log, mut clipboard, mut disk) = world();
        let mut s = shot(40, 20, 2.0);
        let seen = Rc::clone(&log);
        let outcome = run(
            &mut s,
            &Settings::default(),
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |shot, then| {
                // By the time the editor opens, the shot is on the clipboard and on disk.
                assert_eq!(seen.borrow().events, ["copy", "save"]);
                assert!(shot.saved_to.is_some());
                assert_eq!(then, Then::Edit);
            },
        );
        assert_eq!(outcome.then, Then::Edit);
        // Even when the save fails, the editor opens with the shot, so it isn't lost.
        disk.fails = true;
        let mut opened = false;
        let failed = run(
            &mut s,
            &Settings::default(),
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, _| {
                opened = true;
            },
        );
        assert!(opened);
        assert!(!failed.problems.is_empty());
    }

    #[test]
    fn copy_and_save_share_one_encode() -> Result<(), String> {
        let (log, mut clipboard, mut disk) = world();
        let mut s = shot(64, 48, 2.0);
        let outcome = run(
            &mut s,
            &Settings::default(),
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, _| {},
        );
        // Both PNG: the clipboard and the file get the very same bytes.
        let saved = outcome
            .saved_to
            .as_ref()
            .and_then(|p| disk.files.get(p))
            .ok_or("not saved")?;
        assert_eq!(log.borrow().copied.as_ref(), Some(saved));
        // JPEG on disk is its own encode.
        let settings = Settings {
            format: Format::Jpeg,
            ..Settings::default()
        };
        let jpeg = run(
            &mut s,
            &settings,
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, _| {},
        );
        let path = jpeg.saved_to.ok_or("not saved")?;
        assert_ne!(log.borrow().copied.as_ref(), disk.files.get(&path));
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("jpg"));
        Ok(())
    }

    #[test]
    fn a_failed_save_is_shown() {
        let (_log, mut clipboard, mut disk) = world();
        clipboard.works = false;
        disk.fails = true;
        let mut s = shot(10, 10, 1.0);
        let outcome = run(
            &mut s,
            &Settings::default(),
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, _| {},
        );
        assert!(!outcome.copied);
        assert_eq!(outcome.saved_to, None);
        assert_eq!(outcome.problems.len(), 2);
        assert_eq!(outcome.note(), "Couldn't copy the shot");
        let second = outcome.problems.get(1).cloned().unwrap_or_default();
        assert!(
            second.starts_with(&format!(
                "Couldn't save the shot to {}",
                Path::new(HOME).join("Pictures").join("Shots").display()
            )),
            "{second}"
        );
    }

    #[test]
    fn full_resolution_or_one_to_one() -> Result<(), String> {
        let (_log, mut clipboard, mut disk) = world();
        let mut s = shot(300, 150, 1.5);
        let mut settings = Settings {
            after: After::Save,
            retina: false,
            ..Settings::default()
        };
        let outcome = run(
            &mut s,
            &settings,
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, _| {},
        );
        let saved = outcome
            .saved_to
            .as_ref()
            .and_then(|p| disk.files.get(p))
            .ok_or("not saved")?;
        assert_eq!(encoding::size(saved), Some((200, 100)));
        // At full resolution, as captured.
        settings.retina = true;
        let full = run(
            &mut s,
            &settings,
            Path::new(HOME),
            &mut clipboard,
            &mut disk,
            |_, _| {},
        );
        let saved = full
            .saved_to
            .as_ref()
            .and_then(|p| disk.files.get(p))
            .ok_or("not saved")?;
        assert_eq!(encoding::size(saved), Some((300, 150)));
        Ok(())
    }
}
