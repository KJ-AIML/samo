//! One Samo at a time (ADR-006 §3). On the Mac, macOS keeps one copy of an app and its Dock icon brings the window
//! back; Windows does neither, so a second launch asks the first to show its window and quits.
//!
//! The first Samo holds a lock on `samo.lock` for as long as it runs. A second one, finding it locked, leaves a
//! `show.request` beside it, which the first takes on its next wake (about a second). Files only: no socket.
//!
//! Used on Windows; tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

/// The running Samo: it holds the lock until it quits (the lock goes with the file handle).
pub struct Instance {
    _lock: File,
    dir: PathBuf,
}

pub enum Claim {
    /// This is the only Samo.
    Claimed(Instance),
    /// Another Samo runs; it has been asked to show its window.
    AlreadyRunning,
}

impl Instance {
    /// Claims `dir` for this Samo, or, if another holds it, asks that one to show its window.
    pub fn claim(dir: &Path) -> io::Result<Claim> {
        fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(dir.join("samo.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Claim::Claimed(Instance {
                _lock: file,
                dir: dir.to_path_buf(),
            })),
            Err(TryLockError::WouldBlock) => {
                fs::write(dir.join("show.request"), b"show")?;
                Ok(Claim::AlreadyRunning)
            }
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    /// Whether another launch asked this Samo to show its window since last asked. Each request is taken once.
    pub fn take_show_request(&self) -> bool {
        fs::remove_file(self.dir.join("show.request")).is_ok()
    }
}

/// Where the lock lives: `%LOCALAPPDATA%\Samo` on Windows.
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Samo"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_asks_the_first_to_show() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let Claim::Claimed(first) = Instance::claim(tmp.path()).map_err(|e| e.to_string())? else {
            return Err("the first launch should claim".into());
        };
        assert!(!first.take_show_request());

        assert!(matches!(
            Instance::claim(tmp.path()).map_err(|e| e.to_string())?,
            Claim::AlreadyRunning
        ));
        assert!(first.take_show_request());
        assert!(!first.take_show_request(), "a request is taken once");

        drop(first);
        assert!(
            matches!(
                Instance::claim(tmp.path()).map_err(|e| e.to_string())?,
                Claim::Claimed(_)
            ),
            "free again once the first Samo quits"
        );
        Ok(())
    }
}
