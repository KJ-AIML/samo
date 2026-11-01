//! Extensions (ADR-002): what may be installed and run (R5), and how a misbehaving helper is held back (R8).
//! Pure: callers pass in what they read from disk and the time.

use samo_ext_protocol::valid_id;
use std::collections::VecDeque;
use std::fmt;

/// R5: a package holds at most this many entries…
pub const MAX_ENTRIES: usize = 2_000;
/// …and unpacks to at most this many bytes.
pub const MAX_UNPACKED_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Hardlink,
    /// A device, pipe, socket or anything else that isn't a plain file or folder.
    Other,
}

/// One entry of a package (an archive, or a dev folder being copied), with `/`-separated path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtError {
    BadId(String),
    UnsafeEntry { path: String, why: &'static str },
    TooManyEntries(usize),
    TooBig(u64),
}

impl fmt::Display for ExtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadId(id) => write!(f, "`{id}` is not a valid extension id"),
            Self::UnsafeEntry { path, why } => write!(f, "refused `{path}`: {why}"),
            Self::TooManyEntries(n) => {
                write!(f, "{n} entries is more than the {MAX_ENTRIES} allowed")
            }
            Self::TooBig(bytes) => write!(
                f,
                "{bytes} bytes unpacked is more than the {MAX_UNPACKED_BYTES} allowed"
            ),
        }
    }
}

/// R5: every entry is a plain file or folder at a relative path inside `<id>/`, and the whole is small enough.
pub fn validate_entries(id: &str, entries: &[ArchiveEntry]) -> Result<(), ExtError> {
    if !valid_id(id) {
        return Err(ExtError::BadId(id.to_string()));
    }
    if entries.len() > MAX_ENTRIES {
        return Err(ExtError::TooManyEntries(entries.len()));
    }
    let mut total: u64 = 0;
    for entry in entries {
        let unsafe_entry = |why| {
            Err(ExtError::UnsafeEntry {
                path: entry.path.clone(),
                why,
            })
        };
        match entry.kind {
            EntryKind::File | EntryKind::Dir => {}
            EntryKind::Symlink => return unsafe_entry("symbolic links are not allowed"),
            EntryKind::Hardlink => return unsafe_entry("hard links are not allowed"),
            EntryKind::Other => return unsafe_entry("only files and folders are allowed"),
        }
        let path = entry.path.strip_suffix('/').unwrap_or(&entry.path);
        if path.starts_with('/') {
            return unsafe_entry("absolute paths are not allowed");
        }
        if path.contains('\\') || path.contains('\0') {
            return unsafe_entry("the path has a backslash or NUL in it");
        }
        let mut parts = path.split('/');
        if parts.next() != Some(id) {
            return unsafe_entry("everything must be inside the extension's own folder");
        }
        if parts.any(|p| p.is_empty() || p == "." || p == "..") {
            return unsafe_entry("the path has an empty, `.` or `..` part");
        }
        if entry.kind == EntryKind::File {
            total = total.saturating_add(entry.size);
        }
    }
    if total > MAX_UNPACKED_BYTES {
        return Err(ExtError::TooBig(total));
    }
    Ok(())
}

/// Where an installed extension came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// From Samo's signed catalog.
    Catalog,
    /// Copied unsigned from a developer's folder (`SAMO_EXT_DEV_DIR`); `path` is the extension's own folder.
    Dev { path: String },
}

fn trim_slashes(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() { "/" } else { trimmed }
}

/// R5: unsigned code runs only while `SAMO_EXT_DEV_DIR` points at its source (its folder, or the folder holding
/// it), never just because it was installed once.
pub fn may_run(source: &Source, dev_dir_env: Option<&str>) -> bool {
    match source {
        Source::Catalog => true,
        Source::Dev { path } => {
            let Some(dev_dir) = dev_dir_env.map(trim_slashes) else {
                return false;
            };
            let path = trim_slashes(path);
            path == dev_dir
                || path
                    .rsplit_once('/')
                    .map(|(parent, _)| trim_slashes(parent))
                    == Some(dev_dir)
        }
    }
}

/// `major.minor.patch` of a version, ignoring any `-pre` or `+build`; missing parts are 0.
fn numeric(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

/// Whether this Samo (`samo`) is at least `min_samo`. An unreadable version is not satisfied.
pub fn samo_satisfies(min_samo: &str, samo: &str) -> bool {
    match (numeric(min_samo), numeric(samo)) {
        (Some(min), Some(have)) => have >= min,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffConfig {
    /// The wait before the first restart; it doubles with each crash.
    pub first_ms: u64,
    pub max_ms: u64,
    /// This many crashes within `window_ms` stop the helper for good.
    pub max_crashes: usize,
    pub window_ms: i64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            first_ms: 1_000,
            max_ms: 60_000,
            max_crashes: 5,
            window_ms: 10 * 60 * 1_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    After(u64),
    Stop,
}

/// R8: restart a crashed helper after 1, 2, 4 … s; after too many crashes in a short time, stop it.
#[derive(Debug, Clone)]
pub struct Backoff {
    config: BackoffConfig,
    crashes: Vec<i64>,
}

impl Backoff {
    pub fn new(config: BackoffConfig) -> Self {
        Self {
            config,
            crashes: Vec::new(),
        }
    }

    pub fn on_crash(&mut self, now_ms: i64) -> Restart {
        let window = self.config.window_ms;
        self.crashes.retain(|t| now_ms - t < window);
        self.crashes.push(now_ms);
        let n = self.crashes.len();
        if n >= self.config.max_crashes {
            return Restart::Stop;
        }
        let shift = u32::try_from(n.saturating_sub(1))
            .unwrap_or(u32::MAX)
            .min(16);
        Restart::After(
            self.config
                .first_ms
                .saturating_mul(1 << shift)
                .min(self.config.max_ms),
        )
    }

    /// Forgets past crashes, e.g. when the user presses Restart.
    pub fn reset(&mut self) {
        self.crashes.clear();
    }
}

/// R8: lets through at most `per_window` events in any `window_ms`, each at least `min_gap_ms` after the last.
#[derive(Debug, Clone)]
pub struct RateGate {
    min_gap_ms: i64,
    per_window: usize,
    window_ms: i64,
    recent: VecDeque<i64>,
}

impl RateGate {
    pub fn new(min_gap_ms: i64, per_window: usize, window_ms: i64) -> Self {
        Self {
            min_gap_ms,
            per_window,
            window_ms,
            recent: VecDeque::new(),
        }
    }

    /// Messages from a helper: 20 a second.
    pub fn messages() -> Self {
        Self::new(0, 20, 1_000)
    }

    /// Pop-ups from one extension: 3 s apart, 10 a minute.
    pub fn popups() -> Self {
        Self::new(3_000, 10, 60_000)
    }

    pub fn admit(&mut self, now_ms: i64) -> bool {
        while self
            .recent
            .front()
            .is_some_and(|t| now_ms - t >= self.window_ms)
        {
            self.recent.pop_front();
        }
        if self
            .recent
            .back()
            .is_some_and(|last| now_ms - last < self.min_gap_ms)
        {
            return false;
        }
        if self.recent.len() >= self.per_window {
            return false;
        }
        self.recent.push_back(now_ms);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size: u64) -> ArchiveEntry {
        ArchiveEntry {
            path: path.into(),
            kind: EntryKind::File,
            size,
        }
    }

    fn dir(path: &str) -> ArchiveEntry {
        ArchiveEntry {
            path: path.into(),
            kind: EntryKind::Dir,
            size: 0,
        }
    }

    #[test]
    fn extension_rejects_unsafe_entries() {
        let good = vec![
            dir("hello/"),
            file("hello/extension.toml", 400),
            dir("hello/bin"),
            file("hello/bin/samo-ext-hello", 3_000_000),
        ];
        assert_eq!(validate_entries("hello", &good), Ok(()));

        let refused = |entry: ArchiveEntry| {
            let mut entries = good.clone();
            entries.push(entry);
            validate_entries("hello", &entries).is_err()
        };
        assert!(refused(file("/etc/passwd", 1)));
        assert!(refused(file("hello/../../.zshrc", 1)));
        assert!(refused(file("hello/./x", 1)));
        assert!(refused(file("hello//x", 1)));
        assert!(refused(file("other/x", 1)));
        assert!(refused(file("hello\\..\\x", 1)));
        assert!(refused(file("x", 1)));
        assert!(refused(ArchiveEntry {
            path: "hello/link".into(),
            kind: EntryKind::Symlink,
            size: 0
        }));
        assert!(refused(ArchiveEntry {
            path: "hello/hard".into(),
            kind: EntryKind::Hardlink,
            size: 0
        }));
        assert!(refused(ArchiveEntry {
            path: "hello/fifo".into(),
            kind: EntryKind::Other,
            size: 0
        }));

        assert_eq!(
            validate_entries("hello", &[file("hello/big", MAX_UNPACKED_BYTES + 1)]),
            Err(ExtError::TooBig(MAX_UNPACKED_BYTES + 1))
        );
        let many: Vec<_> = (0..=MAX_ENTRIES)
            .map(|i| file(&format!("hello/{i}"), 1))
            .collect();
        assert!(matches!(
            validate_entries("hello", &many),
            Err(ExtError::TooManyEntries(_))
        ));
        assert!(matches!(
            validate_entries("../x", &good),
            Err(ExtError::BadId(_))
        ));
    }

    #[test]
    fn dev_extension_runs_only_with_matching_env() {
        let dev = Source::Dev {
            path: "/Users/kj/samo/target/ext-dev/hello".into(),
        };
        assert!(may_run(&Source::Catalog, None));
        assert!(!may_run(&dev, None));
        assert!(may_run(&dev, Some("/Users/kj/samo/target/ext-dev/hello")));
        assert!(may_run(&dev, Some("/Users/kj/samo/target/ext-dev/hello/")));
        // The folder holding it.
        assert!(may_run(&dev, Some("/Users/kj/samo/target/ext-dev")));
        assert!(may_run(&dev, Some("/Users/kj/samo/target/ext-dev/")));
        // Anywhere else.
        assert!(!may_run(&dev, Some("/Users/kj/samo/target")));
        assert!(!may_run(
            &dev,
            Some("/Users/kj/samo/target/ext-dev/hello-2")
        ));
        assert!(!may_run(&dev, Some("/")));
        assert!(!may_run(&dev, Some("")));
    }

    #[test]
    fn samo_versions() {
        assert!(samo_satisfies("0.1.0", "0.1.0"));
        assert!(samo_satisfies("0.1", "0.1.3"));
        assert!(samo_satisfies("0.1.0", "1.0.0-beta"));
        assert!(!samo_satisfies("0.2.0", "0.1.9"));
        assert!(!samo_satisfies("x", "0.1.0"));
    }

    #[test]
    fn crash_backoff_and_stop() {
        let mut b = Backoff::new(BackoffConfig::default());
        assert_eq!(b.on_crash(0), Restart::After(1_000));
        assert_eq!(b.on_crash(2_000), Restart::After(2_000));
        assert_eq!(b.on_crash(5_000), Restart::After(4_000));
        assert_eq!(b.on_crash(10_000), Restart::After(8_000));
        assert_eq!(b.on_crash(20_000), Restart::Stop);

        // Crashes spread over more than ten minutes never stop it.
        let mut slow = Backoff::new(BackoffConfig::default());
        for i in 0..20 {
            assert_eq!(
                slow.on_crash(i * 4 * 60 * 1_000),
                Restart::After(if i < 3 { 1_000 << i } else { 4_000 })
            );
        }

        // Restart forgets the past.
        b.reset();
        assert_eq!(b.on_crash(30_000), Restart::After(1_000));

        // The wait never passes the cap.
        let mut capped = Backoff::new(BackoffConfig {
            first_ms: 1_000,
            max_ms: 60_000,
            max_crashes: 100,
            window_ms: i64::MAX,
        });
        let last = (0..20).map(|i| capped.on_crash(i)).last();
        assert_eq!(last, Some(Restart::After(60_000)));
    }

    #[test]
    fn popup_gate_spacing_and_cap() {
        let mut gate = RateGate::popups();
        assert!(gate.admit(0));
        assert!(!gate.admit(2_999));
        assert!(gate.admit(3_000));
        // Ten a minute at most, however spaced.
        let mut admitted = 2;
        let mut t = 3_000;
        // Every 3 s for the rest of the first minute.
        while t + 3_000 < 60_000 {
            t += 3_000;
            if gate.admit(t) {
                admitted += 1;
            }
        }
        assert_eq!(admitted, 10);
        // A minute after the first, room again.
        assert!(gate.admit(60_000 + 3_000));
    }

    #[test]
    fn message_gate_twenty_per_second() {
        let mut gate = RateGate::messages();
        let admitted = (0..100).filter(|i| gate.admit(i * 5)).count();
        assert_eq!(admitted, 20);
        assert!(gate.admit(1_000));
    }
}
