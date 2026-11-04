//! What a saved shot is called: `Shot 2026-11-01 at 21.35.12.png`, with ` (2)` if that name is taken. Ported from
//! the Mac's `FileNaming.swift`.

use crate::settings::Format;
use chrono::{DateTime, TimeZone};
use std::fmt::Display;
use std::path::{Path, PathBuf};

/// A shot's name, from when it was taken (in `at`'s time zone: local for shots).
pub fn name<Tz: TimeZone>(at: DateTime<Tz>, format: Format) -> String
where
    Tz::Offset: Display,
{
    let ext = match format {
        Format::Png => "png",
        Format::Jpeg => "jpg",
    };
    format!("Shot {}.{ext}", at.format("%Y-%m-%d at %H.%M.%S"))
}

/// `name` in `folder`, or `name (2)`, `(3)` … if taken; past 999, a unique stamp.
pub fn unique(name: &str, folder: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = folder.join(name);
    if !exists(&first) {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) => (stem, format!(".{ext}")),
        None => (name, String::new()),
    };
    for n in 2..=999 {
        let candidate = folder.join(format!("{stem} ({n}){ext}"));
        if !exists(&candidate) {
            return candidate;
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    folder.join(format!("{stem} {stamp}{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn file_naming() {
        let at =
            chrono::DateTime::<chrono::Utc>::from_timestamp(1_793_543_712, 0).unwrap_or_default();
        assert_eq!(name(at, Format::Png), "Shot 2026-11-01 at 14.35.12.png");
        assert_eq!(name(at, Format::Jpeg), "Shot 2026-11-01 at 14.35.12.jpg");
        let folder = Path::new("/tmp/shots");
        let taken: HashSet<PathBuf> = [folder.join("a.png"), folder.join("a (2).png")].into();
        let exists = |p: &Path| taken.contains(p);
        let named = |n: &str| {
            unique(n, folder, exists)
                .file_name()
                .and_then(|f| f.to_str())
                .map(str::to_string)
        };
        assert_eq!(named("b.png").as_deref(), Some("b.png"));
        assert_eq!(named("a.png").as_deref(), Some("a (3).png"));
        // Thai in a name comes through whole.
        assert_eq!(named("ภาพ.png").as_deref(), Some("ภาพ.png"));
    }
}
