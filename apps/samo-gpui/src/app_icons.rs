//! The icons of apps, and which processes are applications (the Mac's Dock; Windows' taskbar). Both are things only
//! the OS knows, so it is asked, through safe bindings (R29: no `unsafe`), on a worker thread of this module's own
//! and never on the UI thread: a first icon takes a few milliseconds and the list of applications more. The page
//! asks `icon()` and gets what is ready, or nothing and a load queued; the worker wakes the UI when something
//! arrives.
//!
//! - **The Mac:** AppKit, for an app's `.app` folder.
//! - **Windows:** the `.exe`'s own icon, or its Store package's logo (`exe_icon`), and its description as the app's
//!   name. Applications are the processes with a window on the taskbar.

use futures::channel::mpsc::UnboundedSender;
use gpui::Image;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Whether this OS can supply icons and its applications.
pub const ON_THIS_OS: bool = cfg!(any(target_os = "macos", windows));

/// More than a computer has apps running; the cache stops growing here so it can't grow without end.
const MAX_ICONS: usize = 512;

// `Ready` and `Missing` are set by the OS's worker; nothing sets them where there isn't one.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
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
    /// What an app says it is, where the OS tells (Windows: the `.exe`'s description). Set with its icon.
    titles: HashMap<String, String>,
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
    /// Starts the worker (on the Mac and Windows). `wake` is how the UI hears that something arrived.
    pub fn start(wake: UnboundedSender<()>) -> Self {
        let (this, jobs) = Self::unstarted();
        #[cfg(target_os = "macos")]
        mac::spawn(this.state.clone(), jobs, wake);
        #[cfg(windows)]
        win::spawn(this.state.clone(), jobs, wake);
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = (jobs, wake);
        this
    }

    fn unstarted() -> (Self, std::sync::mpsc::Receiver<String>) {
        let (jobs, rx) = std::sync::mpsc::channel();
        let state = Arc::new(Mutex::new(State {
            icons: HashMap::new(),
            titles: HashMap::new(),
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

    /// What the app says it is, once its icon was looked for (Windows: "Antimalware Service Executable" for
    /// `MsMpEng.exe`). The Mac's apps are named by their folder, so there it is always none.
    pub fn title(&self, app: &str) -> Option<String> {
        self.state.lock().ok()?.titles.get(app).cloned()
    }

    /// Whether the OS had no icon for this app (so a gear stands in), as opposed to one still on its way.
    pub fn is_missing(&self, bundle: &str) -> bool {
        self.state
            .lock()
            .is_ok_and(|s| matches!(s.icons.get(bundle), Some(Slot::Missing)))
    }

    /// The PIDs of the OS's applications, and whether the OS has answered yet.
    pub fn applications(&self) -> (Arc<HashSet<u32>>, bool) {
        self.state.lock().map_or_else(
            |_| (Arc::new(HashSet::new()), false),
            |s| (s.applications.clone(), s.known),
        )
    }

    #[cfg(test)]
    fn set_icon(&self, bundle: &str, slot: Slot) {
        if let Ok(mut state) = self.state.lock() {
            state.icons.insert(bundle.to_string(), slot);
        }
    }
}

#[cfg(windows)]
mod win {
    use super::{Slot, State};
    use futures::channel::mpsc::UnboundedSender;
    use gpui::{Image, ImageFormat};
    use std::collections::HashSet;
    use std::path::Path;
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use winsafe::co;
    use winsafe::prelude::Handle;

    pub fn spawn(state: Arc<Mutex<State>>, jobs: Receiver<String>, wake: UnboundedSender<()>) {
        let _ = std::thread::Builder::new()
            .name("samo-icons".into())
            .spawn(move || run(&state, &jobs, &wake));
    }

    fn run(state: &Mutex<State>, jobs: &Receiver<String>, wake: &UnboundedSender<()>) {
        refresh_applications(state, wake);
        loop {
            // A job, or two seconds of quiet, which is when the taskbar's applications are looked at again.
            match jobs.recv_timeout(Duration::from_secs(2)) {
                Ok(exe) => {
                    let path = Path::new(&exe);
                    let slot = crate::exe_icon::icon_png(path).map_or(Slot::Missing, |png| {
                        Slot::Ready(Arc::new(Image::from_bytes(ImageFormat::Png, png)))
                    });
                    let title = crate::exe_icon::description(path);
                    if let Ok(mut s) = state.lock() {
                        if let Some(title) = title {
                            s.titles.insert(exe.clone(), title);
                        }
                        s.icons.insert(exe, slot);
                    }
                    let _ = wake.unbounded_send(());
                }
                Err(RecvTimeoutError::Timeout) => refresh_applications(state, wake),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn refresh_applications(state: &Mutex<State>, wake: &UnboundedSender<()>) {
        let mut pids = HashSet::new();
        let listed = winsafe::EnumWindows(|hwnd: winsafe::HWND| {
            if on_the_taskbar(&hwnd) {
                pids.insert(hwnd.GetWindowThreadProcessId().1);
            }
            true
        });
        if listed.is_err() {
            return;
        }
        let Ok(mut s) = state.lock() else { return };
        if !s.known || *s.applications != pids {
            s.applications = Arc::new(pids);
            s.known = true;
            drop(s);
            let _ = wake.unbounded_send(());
        }
    }

    /// A window the taskbar shows, as Task Manager's "Apps" are counted: visible, owned by no other window, with a
    /// title, not a tool window, and not cloaked (a Store app keeps hidden frames that are "visible").
    fn on_the_taskbar(hwnd: &winsafe::HWND) -> bool {
        if !hwnd.IsWindowVisible() || hwnd.GetWindowTextLength().unwrap_or(0) <= 0 {
            return false;
        }
        if hwnd
            .GetWindow(co::GW::OWNER)
            .is_ok_and(|owner| owner != winsafe::HWND::NULL)
        {
            return false;
        }
        let ex_style = hwnd.GetWindowLongPtr(co::GWLP::EXSTYLE) as u32;
        if ex_style & co::WS_EX::TOOLWINDOW.raw() != 0 {
            return false;
        }
        !matches!(
            hwnd.DwmGetWindowAttribute(co::DWMWA::CLOAKED),
            Ok(winsafe::DwmAttr::Cloaked(c)) if c.raw() != 0
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Asks the real window manager, so it is `--ignored` (a CI runner has no desktop):
        /// `cargo test -p samo-gpui -- --ignored taskbar`.
        #[test]
        #[ignore = "asks Windows for its windows; run by hand on a desktop"]
        fn the_taskbar_has_applications() {
            let state = Mutex::new(State {
                icons: Default::default(),
                titles: Default::default(),
                applications: Arc::new(HashSet::new()),
                known: false,
            });
            let (wake, _rx) = futures::channel::mpsc::unbounded();
            refresh_applications(&state, &wake);
            let s = state.lock().map_err(|_| "poisoned");
            let (known, count) = s
                .map(|s| (s.known, s.applications.len()))
                .unwrap_or((false, 0));
            assert!(known && count > 0, "known {known}, {count} applications");
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

    /// The app's icon as an 8-bit PNG of about `WANT_PX` pixels.
    fn load(bundle: &str) -> Option<Arc<Image>> {
        let tiff = best_picture_tiff(bundle)?;
        let png = png_from_tiff(&tiff)?;
        Some(Arc::new(Image::from_bytes(ImageFormat::Png, png)))
    }

    /// One of the pictures the OS has for the app, as its own small TIFF: the smallest that is big enough, else the
    /// biggest there is. (The PNG call takes an untyped dictionary, which the bindings mark `unsafe`, so it isn't
    /// used; TIFF is the safe way out.)
    fn best_picture_tiff(bundle: &str) -> Option<Vec<u8>> {
        autoreleasepool(|_| {
            let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(bundle));
            let tiff = image.TIFFRepresentation()?;
            let reps = NSBitmapImageRep::imageRepsWithData(&tiff);
            let best = reps.iter().min_by_key(|rep| {
                let px = rep.pixelsWide();
                (px < WANT_PX, if px < WANT_PX { -px } else { px })
            })?;
            let bitmap = best.downcast::<NSBitmapImageRep>().ok()?;
            Some(bitmap.TIFFRepresentation()?.to_vec())
        })
    }

    /// What AppKit hands over is a TIFF that `image` and gpui can't read: most icons are 16-bit *half-float*
    /// ("sample format 3": the wide-gamut picture of a modern Mac), a few are 8-bit, all with premultiplied alpha.
    /// gpui draws only 8-bit with straight alpha, so those icons came out blank. This reads the one layout AppKit
    /// writes (uncompressed, 4 samples, strips) and writes an 8-bit PNG; any other layout is no icon (a gear).
    pub(super) fn png_from_tiff(tiff: &[u8]) -> Option<Vec<u8>> {
        let (width, height, rgba) = rgba8_from_tiff(tiff)?;
        let picture = image::RgbaImage::from_raw(width, height, rgba)?;
        let mut png = Vec::new();
        picture
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        Some(png)
    }

    /// A TIFF's bytes, read in either byte order.
    struct Tiff<'a> {
        bytes: &'a [u8],
        big: bool,
    }

    impl Tiff<'_> {
        fn u16(&self, at: usize) -> Option<u16> {
            let b: [u8; 2] = self.bytes.get(at..at + 2)?.try_into().ok()?;
            Some(if self.big {
                u16::from_be_bytes(b)
            } else {
                u16::from_le_bytes(b)
            })
        }

        fn u32(&self, at: usize) -> Option<u32> {
            let b: [u8; 4] = self.bytes.get(at..at + 4)?.try_into().ok()?;
            Some(if self.big {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            })
        }

        /// The values of a SHORT or LONG tag in the first picture, or none if it isn't there.
        fn tag(&self, wanted: u16) -> Option<Vec<u32>> {
            let ifd = usize::try_from(self.u32(4)?).ok()?;
            let entries = usize::from(self.u16(ifd)?);
            (0..entries).find_map(|i| {
                let at = ifd + 2 + 12 * i;
                if self.u16(at)? != wanted {
                    return None;
                }
                let (kind, count) = (self.u16(at + 2)?, usize::try_from(self.u32(at + 4)?).ok()?);
                let size = match kind {
                    3 => 2,
                    4 => 4,
                    _ => return None,
                };
                // Up to four bytes sit in the entry itself, more at the offset it holds.
                let from = if size * count <= 4 {
                    at + 8
                } else {
                    usize::try_from(self.u32(at + 8)?).ok()?
                };
                (0..count)
                    .map(|n| {
                        let p = from + n * size;
                        if size == 2 {
                            self.u16(p).map(u32::from)
                        } else {
                            self.u32(p)
                        }
                    })
                    .collect()
            })
        }
    }

    /// A half-precision float (IEEE binary16), as the wide-gamut icons store each sample.
    fn half_to_f32(h: u16) -> f32 {
        let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
        let exp = i32::from((h >> 10) & 0x1f);
        let mantissa = f32::from(h & 0x3ff);
        match exp {
            0 => sign * mantissa / 1024.0 * 2f32.powi(-14),
            31 if mantissa == 0.0 => sign * f32::INFINITY,
            31 => f32::NAN,
            _ => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(exp - 15),
        }
    }

    /// Width, height and straight 8-bit RGBA from AppKit's TIFF, or none for any layout but the ones it writes.
    fn rgba8_from_tiff(tiff: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
        let big = match tiff.get(..2)? {
            b"MM" => true,
            b"II" => false,
            _ => return None,
        };
        let t = Tiff { bytes: tiff, big };
        if t.u16(2)? != 42 {
            return None;
        }
        let one = |tag: u16| t.tag(tag).and_then(|v| v.first().copied());
        let (width, height) = (one(256)?, one(257)?);
        // An icon is small; this keeps a damaged file from asking for gigabytes.
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return None;
        }
        let bits = t.tag(258)?;
        let format = t.tag(339).unwrap_or_else(|| vec![1; 4]);
        let supported = t.tag(277)? == [4]
            && one(259).unwrap_or(1) == 1
            && one(284).unwrap_or(1) == 1
            && bits.len() == 4
            && bits.iter().all(|&b| b == bits[0])
            && format.len() == 4
            && format.iter().all(|&f| f == format[0]);
        if !supported {
            return None;
        }
        // 16-bit half-float, or 8-bit unsigned: nothing else is written.
        let half = match (bits[0], format[0]) {
            (16, 3) => true,
            (8, 1) => false,
            _ => return None,
        };
        let premultiplied = one(338) == Some(1);

        let (offsets, counts) = (t.tag(273)?, t.tag(279)?);
        if offsets.len() != counts.len() {
            return None;
        }
        let mut data = Vec::new();
        for (&offset, &count) in offsets.iter().zip(&counts) {
            let from = usize::try_from(offset).ok()?;
            data.extend_from_slice(tiff.get(from..from + usize::try_from(count).ok()?)?);
        }
        let pixels = usize::try_from(width).ok()? * usize::try_from(height).ok()?;
        let per_sample = if half { 2 } else { 1 };
        if data.len() != pixels * 4 * per_sample {
            return None;
        }

        let sample = |i: usize| -> f32 {
            if half {
                let b = [data[2 * i], data[2 * i + 1]];
                let h = if big {
                    u16::from_be_bytes(b)
                } else {
                    u16::from_le_bytes(b)
                };
                half_to_f32(h)
            } else {
                f32::from(data[i]) / 255.0
            }
        };
        // Extended-range values (above 1, below 0) and NaN fall to the nearest displayable value.
        let to8 = |v: f32| -> u8 {
            if v.is_nan() {
                0
            } else {
                (v.clamp(0.0, 1.0) * 255.0).round() as u8
            }
        };
        let mut rgba = Vec::with_capacity(pixels * 4);
        for p in 0..pixels {
            let (r, g, b, a) = (
                sample(4 * p),
                sample(4 * p + 1),
                sample(4 * p + 2),
                sample(4 * p + 3),
            );
            if a <= 0.0 || a.is_nan() {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let un = |c: f32| if premultiplied { c / a } else { c };
            rgba.extend_from_slice(&[to8(un(r)), to8(un(g)), to8(un(b)), to8(a)]);
        }
        Some((width, height, rgba))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Builds the kind of TIFF AppKit writes: the strip first (right after the header), then the entries.
        fn build_tiff(
            big: bool,
            width: u32,
            height: u32,
            bits: u16,
            float: bool,
            associated: u16,
            data: &[u8],
        ) -> Vec<u8> {
            let w16 = |v: u16| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            let w32 = |v: u32| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            let mut out = Vec::new();
            out.extend_from_slice(if big { b"MM" } else { b"II" });
            out.extend_from_slice(&w16(42));
            out.extend_from_slice(&w32(0)); // the entries' offset, set below
            out.extend_from_slice(data);
            let ifd = out.len() as u32;
            out[4..8].copy_from_slice(&w32(ifd));
            let sample_arrays = ifd + 2 + 12 * 12 + 4; // after the entries: BitsPerSample, then SampleFormat
            let entry = |tag: u16, kind: u16, count: u32, value: [u8; 4]| {
                let mut e = Vec::new();
                e.extend_from_slice(&w16(tag));
                e.extend_from_slice(&w16(kind));
                e.extend_from_slice(&w32(count));
                e.extend_from_slice(&value);
                e
            };
            // A SHORT or LONG that fits the entry is stored first in it (left-justified).
            let short = |v: u16| {
                let b = w16(v);
                [b[0], b[1], 0, 0]
            };
            out.extend_from_slice(&w16(12));
            for e in [
                entry(256, 4, 1, w32(width)),
                entry(257, 4, 1, w32(height)),
                entry(258, 3, 4, w32(sample_arrays)),
                entry(259, 3, 1, short(1)),
                entry(262, 3, 1, short(2)),
                entry(273, 4, 1, w32(8)),
                entry(277, 3, 1, short(4)),
                entry(278, 4, 1, w32(height)),
                entry(279, 4, 1, w32(data.len() as u32)),
                entry(284, 3, 1, short(1)),
                entry(338, 3, 1, short(associated)),
                entry(339, 3, 4, w32(sample_arrays + 8)),
            ] {
                out.extend_from_slice(&e);
            }
            out.extend_from_slice(&w32(0)); // no next picture
            for _ in 0..4 {
                out.extend_from_slice(&w16(bits));
            }
            for _ in 0..4 {
                out.extend_from_slice(&w16(if float { 3 } else { 1 }));
            }
            out
        }

        fn halves(big: bool, values: &[u16]) -> Vec<u8> {
            values
                .iter()
                .flat_map(|v| {
                    if big {
                        v.to_be_bytes()
                    } else {
                        v.to_le_bytes()
                    }
                })
                .collect()
        }

        fn decode(png: &[u8]) -> Result<image::RgbaImage, String> {
            image::load_from_memory_with_format(png, image::ImageFormat::Png)
                .map(|i| i.to_rgba8())
                .map_err(|e| e.to_string())
        }

        // Half floats: 1.0 = 0x3C00, 0.5 = 0x3800, 0.25 = 0x3400, 0.125 = 0x3000, 0.0625 = 0x2C00, 1.125 = 0x3C80.
        const SAMPLE_PIXELS: [u16; 16] = [
            0x3400, 0x3000, 0x2C00,
            0x3800, // half transparent: straight is twice the stored colour
            0x0000, 0x0000, 0x0000, 0x0000, // transparent
            0x3C00, 0x0000, 0x3800, 0x3C00, // opaque
            0x3C80, 0x0000, 0x0000, 0x3C00, // opaque and brighter than the display's range
        ];

        fn check_sample_pixels(png: &[u8]) -> Result<(), String> {
            let rgba = decode(png)?;
            assert_eq!((rgba.width(), rgba.height()), (2, 2));
            let at = |x, y| rgba.get_pixel(x, y).0;
            assert_eq!(at(0, 0), [128, 64, 32, 128]);
            assert_eq!(at(1, 0), [0, 0, 0, 0]);
            assert_eq!(at(0, 1), [255, 0, 128, 255]);
            assert_eq!(
                at(1, 1),
                [255, 0, 0, 255],
                "over-range is clamped, not wrapped"
            );
            Ok(())
        }

        /// What most icons are: 16-bit half-float, premultiplied (the kind `image` and gpui can't read at all).
        #[test]
        fn a_half_float_premultiplied_icon_becomes_a_straight_8_bit_png() -> Result<(), String> {
            for big in [true, false] {
                let tiff = build_tiff(big, 2, 2, 16, true, 1, &halves(big, &SAMPLE_PIXELS));
                let png = png_from_tiff(&tiff).ok_or("no PNG")?;
                assert!(png.starts_with(b"\x89PNG"));
                check_sample_pixels(&png)?;
            }
            Ok(())
        }

        /// The other kind: 8-bit unsigned, premultiplied.
        #[test]
        fn an_8_bit_premultiplied_icon_is_made_straight_too() -> Result<(), String> {
            let data = [
                64, 32, 16, 128, 0, 0, 0, 0, 255, 0, 128, 255, 255, 0, 0, 255,
            ];
            let tiff = build_tiff(true, 2, 2, 8, false, 1, &data);
            let rgba = decode(&png_from_tiff(&tiff).ok_or("no PNG")?)?;
            assert_eq!(rgba.get_pixel(0, 0).0, [128, 64, 32, 128]);
            assert_eq!(rgba.get_pixel(1, 0).0, [0, 0, 0, 0]);
            assert_eq!(rgba.get_pixel(0, 1).0, [255, 0, 128, 255]);
            Ok(())
        }

        #[test]
        fn straight_alpha_is_left_alone() -> Result<(), String> {
            // ExtraSamples 2 says the colour is not multiplied by the alpha: 64 stays 64.
            let tiff = build_tiff(true, 1, 1, 8, false, 2, &[64, 32, 16, 128]);
            let rgba = decode(&png_from_tiff(&tiff).ok_or("no PNG")?)?;
            assert_eq!(rgba.get_pixel(0, 0).0, [64, 32, 16, 128]);
            Ok(())
        }

        #[test]
        fn half_floats_are_read_right() {
            assert_eq!(half_to_f32(0x3C00), 1.0);
            assert_eq!(half_to_f32(0x3800), 0.5);
            assert_eq!(half_to_f32(0xC000), -2.0);
            assert_eq!(half_to_f32(0x0000), 0.0);
            assert_eq!(half_to_f32(0x7BFF), 65_504.0);
            assert_eq!(half_to_f32(0x7C00), f32::INFINITY);
            assert!(half_to_f32(0x7E00).is_nan());
            // the smallest subnormal
            assert!((half_to_f32(0x0001) - 5.960_464_5e-8).abs() < 1e-12);
        }

        #[test]
        fn a_layout_other_than_appkits_is_no_icon() {
            let good = build_tiff(true, 2, 2, 16, true, 1, &halves(true, &SAMPLE_PIXELS));
            assert!(png_from_tiff(&good).is_some());
            // 12-bit, 32-bit and signed samples, a picture bigger than any icon, and too little data
            assert!(
                png_from_tiff(&build_tiff(
                    true,
                    2,
                    2,
                    12,
                    true,
                    1,
                    &halves(true, &SAMPLE_PIXELS)
                ))
                .is_none()
            );
            assert!(
                png_from_tiff(&build_tiff(
                    true,
                    2,
                    2,
                    16,
                    false,
                    1,
                    &halves(true, &SAMPLE_PIXELS)
                ))
                .is_none()
            );
            assert!(
                png_from_tiff(&build_tiff(
                    true,
                    4000,
                    4000,
                    16,
                    true,
                    1,
                    &halves(true, &SAMPLE_PIXELS)
                ))
                .is_none()
            );
            assert!(
                png_from_tiff(&build_tiff(
                    true,
                    2,
                    2,
                    16,
                    true,
                    1,
                    &halves(true, &SAMPLE_PIXELS[..8])
                ))
                .is_none()
            );
            // cut off in the middle of its entries
            assert!(png_from_tiff(&good[..good.len() - 40]).is_none());
        }

        #[test]
        fn something_that_is_not_a_picture_is_no_icon() {
            assert_eq!(png_from_tiff(b"not a tiff"), None);
            assert_eq!(png_from_tiff(&[]), None);
        }

        /// Asks the real OS, so it is `--ignored`: `cargo test -p samo-gpui -- --ignored icon_from_the_os`.
        #[test]
        #[ignore = "asks AppKit; run by hand"]
        fn icon_from_the_os() -> Result<(), String> {
            let image = load("/System/Applications/Calculator.app").ok_or("no icon")?;
            assert!(image.bytes().starts_with(b"\x89PNG"), "a PNG");
            // Decoded, it is 8-bit RGBA and not blank: some pixel is opaque and not black.
            let rgba = image::load_from_memory_with_format(image.bytes(), image::ImageFormat::Png)
                .map_err(|e| e.to_string())?
                .to_rgba8();
            assert!(
                rgba.pixels()
                    .any(|p| p[3] == 255 && (p[0] | p[1] | p[2]) > 0),
                "the icon is blank"
            );
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
