//! Where Claude Sessions keeps its files, and where Claude Code keeps its own.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

/// The user's home folder (`%USERPROFILE%` on Windows, where Claude Code keeps `.claude`).
pub fn home() -> Option<PathBuf> {
    home_from(|name| env::var_os(name), cfg!(windows))
}

/// `home`, reading the environment through `get`, for the given OS (ADR-006 R25).
pub fn home_from(get: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let name = if windows { "USERPROFILE" } else { "HOME" };
    get(name).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// The extension's data folder: `SAMO_EXT_DATA_DIR` when Samo runs it, else where Samo keeps it. Claude Code
/// runs the hook and status line without Samo's environment, so they find it from the home folder, or on Windows
/// from `%LOCALAPPDATA%`.
pub fn data_dir() -> Option<PathBuf> {
    data_dir_from(|name| env::var_os(name), cfg!(windows))
}

/// `data_dir`, reading the environment through `get`, for the given OS (ADR-006 R25).
pub fn data_dir_from(get: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let set = |name: &str| get(name).filter(|v| !v.is_empty());
    if let Some(dir) = set("SAMO_EXT_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    if windows {
        return set("LOCALAPPDATA").map(|local| {
            PathBuf::from(local)
                .join("Samo")
                .join("Extensions")
                .join("claude-sessions")
                .join("data")
        });
    }
    home_from(&get, false)
        .map(|h| h.join("Library/Application Support/Samo/Extensions/claude-sessions/data"))
}

/// Where each session's file is.
pub fn sessions_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("sessions"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn paths_follow_the_os() {
        let both = vars(&[
            ("HOME", "/Users/a"),
            ("USERPROFILE", "C:/Users/a"),
            ("LOCALAPPDATA", "C:/Users/a/AppData/Local"),
        ]);
        assert_eq!(home_from(&both, false), Some(PathBuf::from("/Users/a")));
        assert_eq!(home_from(&both, true), Some(PathBuf::from("C:/Users/a")));
        assert_eq!(
            data_dir_from(&both, false),
            Some(
                PathBuf::from("/Users/a")
                    .join("Library/Application Support/Samo/Extensions/claude-sessions/data")
            )
        );
        assert_eq!(
            data_dir_from(&both, true),
            Some(
                PathBuf::from("C:/Users/a/AppData/Local")
                    .join("Samo")
                    .join("Extensions")
                    .join("claude-sessions")
                    .join("data")
            )
        );
        // Samo's own word wins, on both.
        let given = vars(&[("SAMO_EXT_DATA_DIR", "/d"), ("HOME", "/Users/a")]);
        for windows in [false, true] {
            assert_eq!(data_dir_from(&given, windows), Some(PathBuf::from("/d")));
        }
        // Windows has no HOME of its own; the Mac never reads USERPROFILE.
        assert_eq!(home_from(vars(&[("HOME", "/Users/a")]), true), None);
        assert_eq!(
            home_from(vars(&[("USERPROFILE", "C:/Users/a")]), false),
            None
        );
    }
}
