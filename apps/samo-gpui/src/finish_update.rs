//! `samo-gpui --finish-update <old.app> <new.app> <pid> <start_time>` (ADR-003 M5, R14).
//!
//! The *new* Samo's binary runs this once the old Samo has quit. It moves the old app to the Trash (never `rm`),
//! moves the new one into its place, and opens it. If the new one can't be put in place, the old app comes back
//! from the Trash and is opened again, so there is always a working Samo.

use samo_extensions::update::PreparedUpdate;
use samo_extensions::{ExtRoot, History};
use samo_macos::{
    default_user_trash_dir, inspect_node_identity, inspect_process_identity, move_to_trash,
    restore_from_trash,
};
use samo_primitives::ProcessIdentity;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const FLAG: &str = "--finish-update";
/// The longest the new Samo waits for the old one to quit.
const WAIT_FOR_QUIT: Duration = Duration::from_secs(30);

/// Starts the new app's binary in update mode, in its own process group so it outlives this Samo. The caller
/// quits right after.
#[allow(dead_code)] // Settings → Updates calls it (the next task).
pub fn launch(prepared: &PreparedUpdate, old_app: &Path) -> Result<(), String> {
    let me =
        inspect_process_identity(std::process::id()).ok_or("Samo couldn't read its own process")?;
    Command::new(prepared.app_path.join("Contents/MacOS/samo-gpui"))
        .arg(FLAG)
        .arg(old_app)
        .arg(&prepared.app_path)
        .arg(me.pid.to_string())
        .arg(me.start_time_secs.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't start the new Samo: {e}"))
}

/// Update mode, if `args` ask for it: returns the exit code, and Samo doesn't start its window.
pub fn from_args(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some(FLAG) {
        return None;
    }
    let (Some(old), Some(new), Some(pid), Some(start)) = (
        args.get(2),
        args.get(3),
        args.get(4).and_then(|p| p.parse().ok()),
        args.get(5).and_then(|s| s.parse().ok()),
    ) else {
        return Some(2);
    };
    let old_samo = ProcessIdentity {
        pid,
        start_time_secs: start,
    };
    Some(match finish(Path::new(old), Path::new(new), old_samo) {
        Ok(()) => 0,
        Err(()) => 1,
    })
}

fn finish(old: &Path, new: &Path, old_samo: ProcessIdentity) -> Result<(), ()> {
    // R4's identity, so a reused pid can't keep us waiting, or let us start too early.
    let deadline = Instant::now() + WAIT_FOR_QUIT;
    while inspect_process_identity(old_samo.pid) == Some(old_samo) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    let history = ExtRoot::resolve();
    let note = |event: &str, detail: &str| {
        if let Some(root) = &history {
            History::append(root, event, "samo", detail);
        }
    };
    let result = match default_user_trash_dir() {
        Ok(trash_dir) => swap(old, new, |p| trash(p, &trash_dir), restore),
        Err(e) => Err(SwapFailed::NothingChanged(e.to_string())),
    };
    match &result {
        Ok(()) => {
            note("samo-update", &format!("{} replaced", old.display()));
            if let Some(staging) = new.parent() {
                let _ = fs::remove_dir(staging);
            }
        }
        Err(e) => note("samo-update-failed", &e.to_string()),
    }
    // The new Samo on success; the old one, put back, otherwise.
    let _ = Command::new("open").arg(old).status();
    result.map_err(|_| ())
}

fn trash(path: &Path, trash_dir: &Path) -> Result<PathBuf, String> {
    let identity = inspect_node_identity(path).ok_or("it can't be read")?;
    move_to_trash(path, identity, trash_dir)
        .map(|o| PathBuf::from(o.trashed_path))
        .map_err(|e| e.to_string())
}

fn restore(trashed: &Path, original: &Path) -> Result<(), String> {
    restore_from_trash(trashed, original).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapFailed {
    /// The old app couldn't go to the Trash: nothing changed.
    NothingChanged(String),
    /// The new app couldn't be put in place, so the old one came back from the Trash.
    Restored(String),
    /// Neither: the old app is in the Trash at `trashed`.
    Lost { why: String, trashed: PathBuf },
}

impl std::fmt::Display for SwapFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingChanged(why) => {
                write!(f, "the old Samo couldn't be moved ({why}); nothing changed")
            }
            Self::Restored(why) => write!(
                f,
                "the new Samo couldn't be put in place ({why}); the old one is back"
            ),
            Self::Lost { why, trashed } => write!(
                f,
                "the new Samo couldn't be put in place ({why}), and the old one couldn't come back from {}",
                trashed.display()
            ),
        }
    }
}

/// Moves `old` to the Trash and `new` into its place. If that fails, `old` comes back.
pub fn swap(
    old: &Path,
    new: &Path,
    trash: impl Fn(&Path) -> Result<PathBuf, String>,
    restore: impl Fn(&Path, &Path) -> Result<(), String>,
) -> Result<(), SwapFailed> {
    if !new.join("Contents/Info.plist").is_file() {
        return Err(SwapFailed::NothingChanged(
            "the new Samo isn't there".into(),
        ));
    }
    let trashed = trash(old).map_err(SwapFailed::NothingChanged)?;
    let placed = match fs::rename(new, old) {
        Ok(()) => Ok(()),
        // Another volume: copy it across instead.
        Err(e) if e.kind() == ErrorKind::CrossesDevices => Command::new("ditto")
            .arg(new)
            .arg(old)
            .status()
            .map_err(|e| e.to_string())
            .and_then(|s| {
                s.success()
                    .then_some(())
                    .ok_or_else(|| "ditto failed".to_string())
            }),
        Err(e) => Err(e.to_string()),
    };
    match placed {
        Ok(()) => Ok(()),
        Err(why) => {
            // A half-copied app in the way would stop the old one coming back.
            if old.exists() {
                let _ = trash(old);
            }
            match restore(&trashed, old) {
                Ok(()) => Err(SwapFailed::Restored(why)),
                Err(_) => Err(SwapFailed::Lost { why, trashed }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn app(dir: &Path, marker: &str) -> Result<PathBuf, String> {
        let app = dir.join("Samo.app");
        fs::create_dir_all(app.join("Contents")).map_err(|e| e.to_string())?;
        fs::write(app.join("Contents/Info.plist"), marker).map_err(|e| e.to_string())?;
        Ok(app)
    }

    fn marker(app: &Path) -> String {
        fs::read_to_string(app.join("Contents/Info.plist")).unwrap_or_default()
    }

    struct Place {
        _dir: tempfile::TempDir,
        old: PathBuf,
        new: PathBuf,
        trash_dir: PathBuf,
    }

    fn place() -> Result<Place, String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let old = app(&dir.path().join("Applications"), "old")?;
        let new = app(&dir.path().join("Update/0.3.0/unpacked"), "new")?;
        let trash_dir = dir.path().join("Trash");
        Ok(Place {
            _dir: dir,
            old,
            new,
            trash_dir,
        })
    }

    fn trashed(p: &Place) -> Vec<String> {
        fs::read_dir(&p.trash_dir)
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn update_swap_trashes_the_old_app_and_puts_the_new_in_place() -> Result<(), String> {
        let p = place()?;
        swap(&p.old, &p.new, |x| trash(x, &p.trash_dir), restore).map_err(|e| e.to_string())?;
        assert_eq!(marker(&p.old), "new");
        assert!(!p.new.exists());
        assert_eq!(trashed(&p), ["Samo.app"]);
        assert_eq!(marker(&p.trash_dir.join("Samo.app")), "old");
        Ok(())
    }

    #[test]
    fn update_swap_restores_old_app_on_failure() -> Result<(), String> {
        let p = place()?;
        // The new app can't be moved out of its folder, so it can't be put in place.
        let staging = p.new.parent().ok_or("no parent")?.to_path_buf();
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o555))
            .map_err(|e| e.to_string())?;
        let result = swap(&p.old, &p.new, |x| trash(x, &p.trash_dir), restore);
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        assert!(matches!(result, Err(SwapFailed::Restored(_))), "{result:?}");
        assert_eq!(marker(&p.old), "old", "the old Samo is back where it was");
        assert!(trashed(&p).is_empty());
        assert_eq!(marker(&p.new), "new");
        Ok(())
    }

    #[test]
    fn update_swap_changes_nothing_if_the_old_app_cant_be_trashed() -> Result<(), String> {
        let p = place()?;
        let result = swap(&p.old, &p.new, |_| Err("locked".to_string()), restore);
        assert_eq!(result, Err(SwapFailed::NothingChanged("locked".into())));
        assert_eq!(
            (marker(&p.old), marker(&p.new)),
            ("old".into(), "new".into())
        );
        // Nor if the new app isn't there.
        let gone = p.new.with_file_name("Missing.app");
        let result = swap(&p.old, &gone, |x| trash(x, &p.trash_dir), restore);
        assert!(matches!(result, Err(SwapFailed::NothingChanged(_))));
        assert_eq!(marker(&p.old), "old");
        Ok(())
    }

    #[test]
    fn update_mode_args() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(from_args(&args(&["samo-gpui"])), None);
        assert_eq!(from_args(&args(&["samo-gpui", FLAG, "/a.app"])), Some(2));
        assert_eq!(
            from_args(&args(&["samo-gpui", FLAG, "/a", "/b", "x", "1"])),
            Some(2)
        );
    }
}
