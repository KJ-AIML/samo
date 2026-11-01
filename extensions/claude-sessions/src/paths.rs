//! Where Claude Sessions keeps its files, and where Claude Code keeps its own.

use std::env;
use std::path::PathBuf;

/// The user's home folder.
pub fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The extension's data folder: `SAMO_EXT_DATA_DIR` when Samo runs it, else where Samo keeps it. Claude Code
/// runs the hook and status line without Samo's environment, so they find it from the home folder.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("SAMO_EXT_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    home().map(|h| h.join("Library/Application Support/Samo/Extensions/claude-sessions/data"))
}

/// Where each session's file is.
pub fn sessions_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("sessions"))
}
