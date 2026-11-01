use std::env;
use std::path::{Path, PathBuf};

/// The Extensions folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtRoot(PathBuf);

impl ExtRoot {
    /// `SAMO_EXT_ROOT` if set, else `~/Library/Application Support/Samo/Extensions`. `None` without a home.
    pub fn resolve() -> Option<Self> {
        if let Some(root) = env::var_os("SAMO_EXT_ROOT").filter(|r| !r.is_empty()) {
            return Some(Self(PathBuf::from(root)));
        }
        let home = env::var_os("HOME").filter(|h| !h.is_empty())?;
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
        self.current(id).join(helper)
    }
}
