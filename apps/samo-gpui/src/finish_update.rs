//! `samo-gpui --finish-update <old app> <new app> <pid> <start_time>` (ADR-003 M5, R14; ADR-006).
//!
//! The *new* Samo's program runs this once the old Samo has quit. It moves the old app (`Samo.app` on the Mac, Samo's
//! folder on Windows) to the Trash or the Recycle Bin (never deleted), moves the new one into its place, and starts
//! it. If the new one can't be put in place, the old app comes back and is started again, so there is always a
//! working Samo.

use crate::platform::{
    default_user_trash_dir, inspect_node_identity, inspect_process_identity, move_to_trash,
    restore_from_trash,
};
use samo_extensions::update::PreparedUpdate;
use samo_extensions::{ExtRoot, History};
use samo_primitives::ProcessIdentity;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const FLAG: &str = "--finish-update";
/// The longest the new Samo waits for the old one to quit.
const WAIT_FOR_QUIT: Duration = Duration::from_secs(30);

/// The program inside an app: `Samo.app/Contents/MacOS/samo-gpui`, or on Windows `<folder>\Samo.exe`.
fn program_in(app: &Path) -> PathBuf {
    if cfg!(windows) {
        app.join("Samo.exe")
    } else {
        app.join("Contents/MacOS/samo-gpui")
    }
}

/// What says an app is really there before anything moves: its `Info.plist`, or on Windows its `Samo.exe`.
fn marker_in(app: &Path) -> PathBuf {
    if cfg!(windows) {
        app.join("Samo.exe")
    } else {
        app.join("Contents/Info.plist")
    }
}

/// So the started Samo outlives this one: its own process group, and on Windows no console either.
#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Starts the new app's program in update mode, apart from this Samo so it outlives it. The caller quits right
/// after.
pub fn launch(prepared: &PreparedUpdate, old_app: &Path) -> Result<(), String> {
    let me =
        inspect_process_identity(std::process::id()).ok_or("Samo couldn't read its own process")?;
    finisher(prepared, old_app, me)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't start the new Samo: {e}"))
}

/// Where Samo's processes stand while it updates: the temporary folder, never inside an app's folder, which Windows
/// won't move while a process has it as its working folder.
pub fn neutral_dir() -> PathBuf {
    std::env::temp_dir()
}

/// The new Samo's program in update mode, apart from this Samo, standing outside both apps.
fn finisher(prepared: &PreparedUpdate, old_app: &Path, me: ProcessIdentity) -> Command {
    let mut command = Command::new(program_in(&prepared.app_path));
    command
        .arg(FLAG)
        .arg(old_app)
        .arg(&prepared.app_path)
        .arg(me.pid.to_string())
        .arg(me.start_time_secs.to_string())
        .current_dir(neutral_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    command
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
        Ok(trash_dir) => swap(old, new, |p| trash(p, &trash_dir), restore, place_app),
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
    start(old);
    result.map_err(|_| ())
}

/// Starts the Samo at `app`.
fn start(app: &Path) {
    if cfg!(windows) {
        let mut command = Command::new(program_in(app));
        command
            .current_dir(neutral_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detach(&mut command);
        let _ = command.spawn();
    } else {
        let _ = Command::new("open").arg(app).status();
    }
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

/// Moves the new app to where the old one was: a rename, or across volumes a copy (`ditto` on the Mac).
fn place_app(new: &Path, old: &Path) -> Result<(), String> {
    match fs::rename(new, old) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => copy_across(new, old),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(windows))]
fn copy_across(new: &Path, old: &Path) -> Result<(), String> {
    Command::new("ditto")
        .arg(new)
        .arg(old)
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| {
            s.success()
                .then_some(())
                .ok_or_else(|| "ditto failed".to_string())
        })
}

/// On Windows, Samo's folder copied file by file (it holds plain files only: its zip was checked).
#[cfg(windows)]
fn copy_across(new: &Path, old: &Path) -> Result<(), String> {
    fs::create_dir_all(old).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(new).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let to = old.join(entry.file_name());
        if kind.is_dir() {
            copy_across(&entry.path(), &to)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
            Self::Lost { why, trashed } if trashed.as_os_str().is_empty() => write!(
                f,
                "the new Samo couldn't be put in place ({why}), and the old one is in the Trash or the Recycle Bin"
            ),
            Self::Lost { why, trashed } => write!(
                f,
                "the new Samo couldn't be put in place ({why}), and the old one couldn't come back from {}",
                trashed.display()
            ),
        }
    }
}

/// Moves `old` to the Trash and `new` into its place (`place`). If that fails, `old` comes back.
pub fn swap(
    old: &Path,
    new: &Path,
    trash: impl Fn(&Path) -> Result<PathBuf, String>,
    restore: impl Fn(&Path, &Path) -> Result<(), String>,
    place: impl Fn(&Path, &Path) -> Result<(), String>,
) -> Result<(), SwapFailed> {
    if !marker_in(new).is_file() {
        return Err(SwapFailed::NothingChanged(
            "the new Samo isn't there".into(),
        ));
    }
    // The Recycle Bin can move the folder and then fail to find it there: the old Samo is gone either way, so the
    // new one goes in, or no Samo would start.
    let trashed = match trash(old) {
        Ok(path) => Some(path),
        Err(why) if old.exists() => return Err(SwapFailed::NothingChanged(why)),
        Err(_) => None,
    };
    match place(new, old) {
        Ok(()) => Ok(()),
        Err(why) => {
            // A half-copied app in the way would stop the old one coming back.
            if old.exists() {
                let _ = trash(old);
            }
            match &trashed {
                Some(path) if restore(path, old).is_ok() => Err(SwapFailed::Restored(why)),
                _ => Err(SwapFailed::Lost {
                    why,
                    trashed: trashed.unwrap_or_default(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny app at `dir`, the way this OS lays one out, marked `marker`.
    fn app(dir: &Path, marker: &str) -> Result<PathBuf, String> {
        let app = dir.join(if cfg!(windows) { "Samo" } else { "Samo.app" });
        let file = marker_in(&app);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(file, marker).map_err(|e| e.to_string())?;
        Ok(app)
    }

    fn marker(app: &Path) -> String {
        fs::read_to_string(marker_in(app)).unwrap_or_default()
    }

    struct Place {
        _dir: tempfile::TempDir,
        old: PathBuf,
        new: PathBuf,
        trash_dir: PathBuf,
    }

    fn place() -> Result<Place, String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let old = app(&dir.path().join("Programs"), "old")?;
        let new = app(&dir.path().join("Update/0.3.0/unpacked"), "new")?;
        let trash_dir = dir.path().join("Trash");
        fs::create_dir_all(&trash_dir).map_err(|e| e.to_string())?;
        Ok(Place {
            _dir: dir,
            old,
            new,
            trash_dir,
        })
    }

    /// A stand-in for the Trash or the Recycle Bin: a folder, so the tests never touch the real one.
    fn fake_trash(p: &Place) -> impl Fn(&Path) -> Result<PathBuf, String> + '_ {
        |path| {
            let name = path.file_name().ok_or("no name")?;
            let to = p.trash_dir.join(name);
            fs::rename(path, &to).map_err(|e| e.to_string())?;
            Ok(to)
        }
    }

    fn fake_restore(trashed: &Path, original: &Path) -> Result<(), String> {
        fs::rename(trashed, original).map_err(|e| e.to_string())
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
    fn the_new_samo_takes_the_old_ones_place() -> Result<(), String> {
        let p = place()?;
        swap(&p.old, &p.new, fake_trash(&p), fake_restore, place_app).map_err(|e| e.to_string())?;
        assert_eq!(marker(&p.old), "new");
        assert!(!p.new.exists());
        assert_eq!(trashed(&p).len(), 1);
        Ok(())
    }

    #[test]
    fn if_the_new_samo_cant_be_placed_the_old_one_comes_back() -> Result<(), String> {
        let p = place()?;
        let result = swap(&p.old, &p.new, fake_trash(&p), fake_restore, |_, _| {
            Err("in use".to_string())
        });
        assert_eq!(result, Err(SwapFailed::Restored("in use".into())));
        assert_eq!(marker(&p.old), "old", "the old Samo is back where it was");
        assert!(trashed(&p).is_empty());
        assert_eq!(marker(&p.new), "new");
        Ok(())
    }

    #[test]
    fn nothing_changes_if_the_old_samo_cant_be_moved() -> Result<(), String> {
        let p = place()?;
        let result = swap(
            &p.old,
            &p.new,
            |_| Err("locked".to_string()),
            fake_restore,
            place_app,
        );
        assert_eq!(result, Err(SwapFailed::NothingChanged("locked".into())));
        assert_eq!(
            (marker(&p.old), marker(&p.new)),
            ("old".into(), "new".into())
        );
        // Nor if the new app isn't there.
        let gone = p.new.with_file_name("Missing");
        let result = swap(&p.old, &gone, fake_trash(&p), fake_restore, place_app);
        assert!(matches!(result, Err(SwapFailed::NothingChanged(_))));
        assert_eq!(marker(&p.old), "old");
        Ok(())
    }

    #[test]
    fn the_new_samo_goes_in_even_if_the_bin_says_no_after_moving() -> Result<(), String> {
        // The Recycle Bin can move the folder and then fail to find it there. The old Samo is gone either way, so
        // the new one must take its place, or no Samo would start.
        let p = place()?;
        let moved_then_failed = |path: &Path| -> Result<PathBuf, String> {
            let to = p.trash_dir.join("moved");
            fs::rename(path, &to).map_err(|e| e.to_string())?;
            Err("moved, but can't be found in the Recycle Bin".to_string())
        };
        swap(&p.old, &p.new, moved_then_failed, fake_restore, place_app)
            .map_err(|e| e.to_string())?;
        assert_eq!(marker(&p.old), "new");
        Ok(())
    }

    /// On the Mac, the real Trash (in a temporary folder) and a real failure to move.
    #[cfg(target_os = "macos")]
    #[test]
    fn update_swap_with_the_macs_trash() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let p = place()?;
        swap(
            &p.old,
            &p.new,
            |x| trash(x, &p.trash_dir),
            restore,
            place_app,
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(marker(&p.old), "new");
        assert_eq!(trashed(&p), ["Samo.app"]);

        let p = place()?;
        let staging = p.new.parent().ok_or("no parent")?.to_path_buf();
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o555))
            .map_err(|e| e.to_string())?;
        let result = swap(
            &p.old,
            &p.new,
            |x| trash(x, &p.trash_dir),
            restore,
            place_app,
        );
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        assert!(matches!(result, Err(SwapFailed::Restored(_))), "{result:?}");
        assert_eq!(marker(&p.old), "old");
        assert!(trashed(&p).is_empty());
        Ok(())
    }

    #[test]
    fn the_finisher_stands_outside_both_apps() -> Result<(), String> {
        // Windows won't move a folder a running process has as its working folder, so neither the finisher nor the
        // Samo it starts may stand in the old or the new app.
        let prepared = PreparedUpdate {
            version: "0.5.0".into(),
            app_path: PathBuf::from(r"C:\Users\Jo\AppData\Local\Samo\Update\0.5.0\unpacked\Samo"),
        };
        let old = Path::new(r"C:\Users\Jo\AppData\Local\Programs\Samo");
        let me = ProcessIdentity {
            pid: 1,
            start_time_secs: 2,
        };
        let command = finisher(&prepared, old, me);
        let dir = command.get_current_dir().ok_or("no working folder")?;
        assert!(
            !dir.starts_with(old) && !dir.starts_with(&prepared.app_path),
            "{dir:?}"
        );
        assert_eq!(dir, neutral_dir());
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
