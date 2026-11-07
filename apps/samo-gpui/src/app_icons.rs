//! The icons of apps, and which processes are applications in the Dock (macOS). Both are things only the OS knows,
//! so AppKit is asked, through safe bindings (R29: no `unsafe`), on a worker thread of this module's own and never
//! on the UI thread: a first icon takes a few milliseconds and the list of applications more. The page asks
//! `icon()` and gets what is ready, or nothing and a load queued; the worker wakes the UI when something arrives.
//!
//! Windows has none of this yet. `ON_THIS_OS` says so and the page draws no icons there (ADR-006 R28: said, never
//! faked).

use futures::channel::mpsc::UnboundedSender;
use gpui::Image;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Whether this OS can supply icons and the Dock's applications.
pub const ON_THIS_OS: bool = cfg!(target_os = "macos");

/// More than a Mac has apps running; the cache stops growing here so it can't grow without end.
const MAX_ICONS: usize = 512;

#[derive(Clone)]
enum Slot {
    /// Asked for; the worker has it.
    Loading,
    /// The OS had none (or a read failed). Not asked again.
    Missing,
    Ready(Arc<Image>),
}

struct State {
    icons: HashMap<String, Slot>,
    /// PIDs of the OS's applications (the Dock's).
    applications: Arc<HashSet<u32>>,
    /// The first answer has arrived, so an empty `applications` means "none" and not "not known yet".
    known: bool,
}

#[derive(Clone)]
pub struct AppIcons {
    state: Arc<Mutex<State>>,
    jobs: Sender<String>,
}

impl AppIcons {
    /// Starts the worker (on the Mac). `wake` is how the UI hears that something arrived.
    pub fn start(wake: UnboundedSender<()>) -> Self {
        let (this, jobs) = Self::unstarted();
        #[cfg(target_os = "macos")]
        mac::spawn(this.state.clone(), jobs, wake);
        #[cfg(not(target_os = "macos"))]
        let _ = (jobs, wake);
        this
    }

    fn unstarted() -> (Self, std::sync::mpsc::Receiver<String>) {
        let (jobs, rx) = std::sync::mpsc::channel();
        let state = Arc::new(Mutex::new(State {
            icons: HashMap::new(),
            applications: Arc::new(HashSet::new()),
            known: false,
        }));
        (Self { state, jobs }, rx)
    }

    /// The icon of the app in folder `bundle`, if it is ready. The first ask queues the load and says nothing yet;
    /// asking again is cheap, so a page can ask on every frame.
    pub fn icon(&self, bundle: &str) -> Option<Arc<Image>> {
        let mut state = self.state.lock().ok()?;
        match state.icons.get(bundle) {
            Some(Slot::Ready(image)) => return Some(image.clone()),
            Some(Slot::Loading | Slot::Missing) => return None,
            None => {}
        }
        if !ON_THIS_OS || state.icons.len() >= MAX_ICONS {
            return None;
        }
        state.icons.insert(bundle.to_string(), Slot::Loading);
        drop(state);
        let _ = self.jobs.send(bundle.to_string());
        None
    }

    /// The PIDs of the OS's applications, and whether the OS has answered yet.
    pub fn applications(&self) -> (Arc<HashSet<u32>>, bool) {
        self.state.lock().map_or_else(
            |_| (Arc::new(HashSet::new()), false),
            |s| (s.applications.clone(), s.known),
        )
    }

    fn set_icon(&self, bundle: &str, slot: Slot) {
        if let Ok(mut state) = self.state.lock() {
            state.icons.insert(bundle.to_string(), slot);
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{Slot, State};
    use futures::channel::mpsc::UnboundedSender;
    use gpui::{Image, ImageFormat};
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSApplicationActivationPolicy, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::NSString;
    use std::collections::HashSet;
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Icons are shown at up to 28 pt, so 56 px on a Retina screen: the smallest picture of at least this many
    /// pixels is never stretched, and is small (a 1024 px one would be 4 MB of texture for an icon).
    const WANT_PX: isize = 64;

    pub fn spawn(state: Arc<Mutex<State>>, jobs: Receiver<String>, wake: UnboundedSender<()>) {
        let _ = std::thread::Builder::new()
            .name("samo-icons".into())
            .spawn(move || run(&state, &jobs, &wake));
    }

    fn run(state: &Mutex<State>, jobs: &Receiver<String>, wake: &UnboundedSender<()>) {
        refresh_applications(state, wake);
        loop {
            // A job, or two seconds of quiet, which is when the list of applications is looked at again.
            match jobs.recv_timeout(Duration::from_secs(2)) {
                Ok(bundle) => {
                    let slot = load(&bundle).map_or(Slot::Missing, Slot::Ready);
                    if let Ok(mut s) = state.lock() {
                        s.icons.insert(bundle, slot);
                    }
                    let _ = wake.unbounded_send(());
                }
                Err(RecvTimeoutError::Timeout) => refresh_applications(state, wake),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn refresh_applications(state: &Mutex<State>, wake: &UnboundedSender<()>) {
        let pids: HashSet<u32> = autoreleasepool(|_| {
            NSWorkspace::sharedWorkspace()
                .runningApplications()
                .iter()
                .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
                .filter_map(|app| u32::try_from(app.processIdentifier()).ok())
                .collect()
        });
        let Ok(mut s) = state.lock() else { return };
        if !s.known || *s.applications != pids {
            s.applications = Arc::new(pids);
            s.known = true;
            drop(s);
            let _ = wake.unbounded_send(());
        }
    }

    /// The app's icon as one picture of about `WANT_PX` pixels, chosen from the pictures the OS has for it.
    fn load(bundle: &str) -> Option<Arc<Image>> {
        autoreleasepool(|_| {
            let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(bundle));
            let tiff = image.TIFFRepresentation()?;
            let reps = NSBitmapImageRep::imageRepsWithData(&tiff);
            // The smallest picture that is big enough, else the biggest there is.
            let best = reps.iter().min_by_key(|rep| {
                let px = rep.pixelsWide();
                (px < WANT_PX, if px < WANT_PX { -px } else { px })
            })?;
            let bitmap = best.downcast::<NSBitmapImageRep>().ok()?;
            // That one picture alone, as a TIFF (the PNG call takes an untyped dictionary, which the bindings
            // mark `unsafe`, so it isn't used); gpui decodes TIFF.
            let tiff = bitmap.TIFFRepresentation()?;
            Some(Arc::new(Image::from_bytes(
                ImageFormat::Tiff,
                tiff.to_vec(),
            )))
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Asks the real OS, so it is `--ignored`: `cargo test -p samo-gpui -- --ignored icon_from_the_os`.
        #[test]
        #[ignore = "asks AppKit; run by hand"]
        fn icon_from_the_os() -> Result<(), String> {
            let image = load("/System/Applications/Calculator.app").ok_or("no icon")?;
            let magic = &image.bytes()[..4];
            assert!(magic == b"II*\0" || magic == b"MM\0*", "a TIFF: {magic:?}");
            assert!(
                (1_000..400_000).contains(&image.bytes().len()),
                "a small picture, not a megabyte: {} bytes",
                image.bytes().len()
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::ImageFormat;

    fn ready() -> Slot {
        Slot::Ready(Arc::new(Image::from_bytes(ImageFormat::Png, vec![1, 2, 3])))
    }

    #[test]
    fn the_first_ask_queues_one_load_and_later_asks_do_not() {
        if !ON_THIS_OS {
            return;
        }
        let (icons, jobs) = AppIcons::unstarted();
        assert!(icons.icon("/Applications/A.app").is_none());
        assert!(icons.icon("/Applications/A.app").is_none());
        assert!(icons.icon("/Applications/B.app").is_none());
        let queued: Vec<String> = jobs.try_iter().collect();
        assert_eq!(queued, ["/Applications/A.app", "/Applications/B.app"]);
    }

    #[test]
    fn a_ready_icon_is_handed_out_and_a_missing_one_is_not_asked_for_again() {
        if !ON_THIS_OS {
            return;
        }
        let (icons, jobs) = AppIcons::unstarted();
        let _ = icons.icon("/Applications/A.app");
        let _ = icons.icon("/Applications/B.app");
        icons.set_icon("/Applications/A.app", ready());
        icons.set_icon("/Applications/B.app", Slot::Missing);
        assert!(icons.icon("/Applications/A.app").is_some());
        assert!(icons.icon("/Applications/B.app").is_none());
        assert_eq!(
            jobs.try_iter().count(),
            2,
            "nothing was queued a second time"
        );
    }

    #[test]
    fn the_cache_does_not_grow_without_end() {
        if !ON_THIS_OS {
            return;
        }
        let (icons, jobs) = AppIcons::unstarted();
        for i in 0..MAX_ICONS + 50 {
            let _ = icons.icon(&format!("/Applications/App{i}.app"));
        }
        assert_eq!(jobs.try_iter().count(), MAX_ICONS);
    }

    #[test]
    fn applications_are_unknown_until_the_os_answers() {
        let (icons, _jobs) = AppIcons::unstarted();
        let (pids, known) = icons.applications();
        assert!(pids.is_empty() && !known);
    }
}
