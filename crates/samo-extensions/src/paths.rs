use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The Extensions folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtRoot(PathBuf);

impl ExtRoot {
    /// `SAMO_EXT_ROOT` if set, else `~/Library/Application Support/Samo/Extensions`, or on Windows
    /// `%LOCALAPPDATA%\Samo\Extensions` (ADR-006 §3). `None` without a home.
    pub fn resolve() -> Option<Self> {
        Self::resolve_from(|name| env::var_os(name), cfg!(windows))
    }

    /// `resolve`, reading the environment through `get`, for the given OS (ADR-006 R25).
    pub fn resolve_from(get: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<Self> {
        let set = |name: &str| get(name).filter(|v| !v.is_empty());
        if let Some(root) = set("SAMO_EXT_ROOT") {
            return Some(Self(PathBuf::from(root)));
        }
        if windows {
            let local = set("LOCALAPPDATA")?;
            return Some(Self(PathBuf::from(local).join("Samo").join("Extensions")));
        }
        let home = set("HOME")?;
        Some(Self(
            PathBuf::from(home).join("Library/Application Support/Samo/Extensions"),
        ))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn registry_file(&self) -> PathBuf {
        self.0.join("installed.toml")
    }

    pub fn history_file(&self) -> PathBuf {
        self.0.join("history.log")
    }

    pub fn ext_dir(&self, id: &str) -> PathBuf {
        self.0.join(id)
    }

    pub fn version_dir(&self, id: &str, version: &str) -> PathBuf {
        self.ext_dir(id).join(version)
    }

    /// The link to the version in use. Hooks and status lines point through it, so they survive updates.
    pub fn current(&self, id: &str) -> PathBuf {
        self.ext_dir(id).join("current")
    }

    /// The extension's own folder, kept across updates.
    pub fn data_dir(&self, id: &str) -> PathBuf {
        self.ext_dir(id).join("data")
    }

    /// The helper program, through `current`.
    pub fn helper(&self, id: &str, helper: &str) -> PathBuf {
        self.current(id).join(helper_file_on(helper, cfg!(windows)))
    }
}

/// The helper's file as it is on disk: a manifest names `bin/samo-claude` once, and on Windows that program is
/// `bin/samo-claude.exe`.
pub fn helper_file_on(helper: &str, windows: bool) -> String {
    let has_exe = helper
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("exe"));
    if windows && !has_exe {
        format!("{helper}.exe")
    } else {
        helper.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn ext_root_follows_the_os() {
        let both = vars(&[
            ("HOME", "/Users/a"),
            ("LOCALAPPDATA", r"C:\Users\a\AppData\Local"),
        ]);
        assert_eq!(
            ExtRoot::resolve_from(&both, false).map(|r| r.0),
            Some(PathBuf::from("/Users/a").join("Library/Application Support/Samo/Extensions"))
        );
        assert_eq!(
            ExtRoot::resolve_from(&both, true).map(|r| r.0),
            Some(
                PathBuf::from(r"C:\Users\a\AppData\Local")
                    .join("Samo")
                    .join("Extensions")
            )
        );
        let chosen = vars(&[("SAMO_EXT_ROOT", "/x"), ("HOME", "/Users/a")]);
        for windows in [false, true] {
            assert_eq!(
                ExtRoot::resolve_from(&chosen, windows).map(|r| r.0),
                Some(PathBuf::from("/x"))
            );
            assert_eq!(ExtRoot::resolve_from(vars(&[]), windows), None);
        }
        // Windows has no HOME of its own, and the Mac never reads LOCALAPPDATA.
        assert_eq!(
            ExtRoot::resolve_from(vars(&[("HOME", "/Users/a")]), true),
            None
        );
        assert_eq!(
            ExtRoot::resolve_from(vars(&[("LOCALAPPDATA", r"C:\L")]), false),
            None
        );
    }

    #[test]
    fn helpers_are_exes_on_windows() {
        assert_eq!(
            helper_file_on("bin/samo-claude", true),
            "bin/samo-claude.exe"
        );
        assert_eq!(helper_file_on("bin/x.EXE", true), "bin/x.EXE");
        assert_eq!(helper_file_on("bin/samo-claude", false), "bin/samo-claude");
        assert_eq!(
            ExtRoot::at("/r").helper("hello", "bin/hello"),
            PathBuf::from("/r/hello/current").join(helper_file_on("bin/hello", cfg!(windows)))
        );
    }
}
