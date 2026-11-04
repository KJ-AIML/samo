//! R2 on Windows (ADR-006 R27): cleanup goes to the Recycle Bin, and Undo brings back the same item. Samo doesn't
//! delete for good: what the Recycle Bin can't take (no bin, bin off, too big, or a volume it can't vouch for) is
//! refused before anything moves.

use crate::identity::{canonical_path, inspect_node_identity};
use crate::scanner::allocated_file_bytes;
use crate::tools::{CREATE_NO_WINDOW, system_tool};
use samo_domain::{normalize_path, validate_trash_target};
use samo_primitives::{ErrorCode, FileNodeIdentity, SamoError, SamoResult};
use std::collections::HashSet;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use sysinfo::Disks;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashOutcome {
    pub original_path: String,
    /// The Recycle Bin item's id, which `restore_from_trash` takes back.
    pub trashed_path: String,
    pub reclaimed_bytes: u64,
}

/// Windows keeps a Recycle Bin on each drive, so there is no one folder to name; this is what it is called. Samo
/// never uses it as a path on Windows.
pub fn default_user_trash_dir() -> SamoResult<PathBuf> {
    Ok(PathBuf::from("Recycle Bin"))
}

/// The bytes `target` takes on disk, each file once.
fn compute_path_allocated_bytes(target: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(target) else {
        return 0;
    };
    // A link takes next to nothing; what it points to stays where it is.
    if meta.file_type().is_symlink() {
        return 0;
    }
    if meta.is_file() {
        return allocated_file_bytes(target, &meta);
    }
    let mut sum: u64 = 0;
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut stack = vec![target.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let Ok(m) = entry.metadata() else { continue };
            if m.file_type().is_symlink() {
                continue;
            }
            let path = entry.path();
            if m.is_dir() {
                stack.push(path);
            } else if m.is_file()
                && let Some(id) = inspect_node_identity(&path)
                && seen.insert((id.device_id, id.inode))
            {
                sum = sum.saturating_add(allocated_file_bytes(&path, &m));
            }
        }
    }
    sum
}

/// `inside` (in the one shape) is on `mount` or below it, ignoring case as NTFS does.
fn is_on(inside: &str, mount: &str) -> bool {
    let inside = inside.to_ascii_lowercase();
    let mount = mount.to_ascii_lowercase();
    inside == mount || inside.starts_with(&format!("{}/", mount.trim_end_matches('/')))
}

/// What Samo knows about the Recycle Bin of the volume a target is on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BinFacts {
    /// The volume is one `sysinfo` lists.
    listed: bool,
    removable: bool,
    file_system: String,
    /// "Don't move files to the Recycle Bin" for this volume (`NukeOnDelete`).
    nuke_on_delete: Option<u32>,
    /// The most this volume's Recycle Bin holds, in MB (`MaxCapacity`).
    max_capacity_mb: Option<u32>,
}

/// R2 on Windows, before anything moves (ADR-006 R27). Windows deletes for good what its Recycle Bin can't take, so
/// Samo cleans only on a fixed NTFS volume it can see (NTFS keeps a file's index stable, which R2's identity check
/// needs), whose Recycle Bin is on and holds at least `bytes`. Anything Samo can't tell is a no.
fn bin_verdict(facts: &BinFacts, bytes: u64) -> Result<(), String> {
    if !facts.listed {
        return Err(
            "Samo can't see this drive, so it can't be sure of its Recycle Bin".to_string(),
        );
    }
    if facts.removable {
        return Err("this is a removable drive, which has no Recycle Bin".to_string());
    }
    if !facts.file_system.eq_ignore_ascii_case("NTFS") {
        return Err(format!(
            "this drive is {}; Samo cleans only NTFS drives, where it can check a file is the one it measured",
            facts.file_system
        ));
    }
    match (facts.nuke_on_delete, facts.max_capacity_mb) {
        (Some(0), Some(max_mb)) => {
            let max_bytes = u64::from(max_mb).saturating_mul(1024 * 1024);
            if bytes > max_bytes {
                Err(format!(
                    "it is bigger than this drive's Recycle Bin holds ({max_mb} MB), so Windows would delete it for good"
                ))
            } else {
                Ok(())
            }
        }
        (Some(_), _) => Err("this drive's Recycle Bin is turned off".to_string()),
        _ => Err(
            "Samo can't read this drive's Recycle Bin settings; open the Recycle Bin's Properties once and try again"
                .to_string(),
        ),
    }
}

/// The volume GUID (`{…}`) of drive `X:`, as Windows' own `mountvol X:\ /L` names it (`\\?\Volume{…}\`). The
/// Recycle Bin's settings are kept under that GUID.
fn volume_guid(drive: &str) -> Option<String> {
    let out = Command::new(system_tool("mountvol.exe"))
        .arg(format!(r"{drive}\"))
        .arg("/L")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let start = text.find("Volume{")? + "Volume".len();
    let end = start + text.get(start..)?.find('}')? + 1;
    text.get(start..end).map(str::to_string)
}

/// `(NukeOnDelete, MaxCapacity in MB)` of the volume's Recycle Bin, as the user set them in its Properties.
fn bin_settings(guid: &str) -> (Option<u32>, Option<u32>) {
    let Ok(key) = windows_registry::CURRENT_USER.open(format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume\{guid}"
    )) else {
        return (None, None);
    };
    (
        key.get_u32("NukeOnDelete").ok(),
        key.get_u32("MaxCapacity").ok(),
    )
}

/// What Samo can find out about the Recycle Bin of the volume `path` (in the one shape) is on. Only a volume
/// mounted at a drive letter counts, so its settings are the ones read.
fn bin_facts(path: &str) -> BinFacts {
    let disks = Disks::new_with_refreshed_list();
    let volume = disks
        .list()
        .iter()
        .map(|d| (d, normalize_path(&d.mount_point().to_string_lossy())))
        .filter(|(_, mount)| is_on(path, mount))
        .max_by_key(|(_, mount)| mount.len());
    let Some((disk, mount)) = volume else {
        return BinFacts {
            listed: false,
            removable: false,
            file_system: String::new(),
            nuke_on_delete: None,
            max_capacity_mb: None,
        };
    };
    let at_drive_letter = mount.len() == 3 && mount.ends_with(":/");
    let (nuke_on_delete, max_capacity_mb) = match mount.get(..2) {
        Some(drive) if at_drive_letter => volume_guid(drive)
            .map(|guid| bin_settings(&guid))
            .unwrap_or((None, None)),
        _ => (None, None),
    };
    BinFacts {
        listed: true,
        removable: disk.is_removable(),
        file_system: disk.file_system().to_string_lossy().into_owned(),
        nuke_on_delete,
        max_capacity_mb,
    }
}

/// A Recycle Bin call, on a thread of its own: COM starts fresh there whatever the caller's thread uses, and a
/// panic inside the `trash` crate comes back as an error rather than taking Samo down.
fn on_own_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, trash::Error> + Send + 'static,
) -> SamoResult<T> {
    std::thread::Builder::new()
        .name("samo-recycle-bin".to_string())
        .spawn(work)
        .map_err(|e| {
            SamoError::new(
                ErrorCode::InternalError,
                format!("couldn't start the Recycle Bin call: {e}"),
            )
        })?
        .join()
        .map_err(|_| {
            SamoError::new(
                ErrorCode::InternalError,
                "the Recycle Bin call stopped unexpectedly",
            )
        })?
        .map_err(|e| SamoError::new(ErrorCode::InternalError, format!("Recycle Bin: {e}")))
}

/// R2 + RF2 on Windows: checks the target's canonical path against R2 and its `(volume serial, file index)` against
/// the scan, then checks its drive's Recycle Bin can take it, and only then moves it there. Samo doesn't delete for
/// good: what the Recycle Bin can't take is refused before anything moves, and the moved item is found again by its
/// identity, so Undo brings back this item and no other. `_trash_dir` is the Mac's; Windows has a bin per drive.
/// Whether `move_to_trash` would take `target_path` now, checked without moving anything: it is there, R2 allows it
/// (R27), and its drive's Recycle Bin is on and can hold it. Samo asks this before it quits to update itself, so a
/// refusal is said while Samo still runs (R14).
pub fn can_recycle(target_path: &Path) -> Result<(), String> {
    let canonical = canonical_path(target_path)
        .ok_or_else(|| format!("`{}` isn't there", target_path.display()))?;
    let target = PathBuf::from(&canonical);
    let identity =
        inspect_node_identity(&target).ok_or_else(|| format!("`{canonical}` can't be read"))?;
    validate_trash_target(&canonical, identity, Some(identity)).map_err(|e| e.to_string())?;
    bin_verdict(
        &bin_facts(&canonical),
        compute_path_allocated_bytes(&target),
    )
}

pub fn move_to_trash(
    target_path: &Path,
    scanned_identity: FileNodeIdentity,
    _trash_dir: &Path,
) -> SamoResult<TrashOutcome> {
    let Some(canonical) = canonical_path(target_path) else {
        return Err(SamoError::new(
            ErrorCode::NotFound,
            format!(
                "cleanup target `{}` no longer exists",
                target_path.display()
            ),
        ));
    };
    let target = PathBuf::from(&canonical);
    validate_trash_target(&canonical, scanned_identity, inspect_node_identity(&target))?;
    let Some(parent) = target
        .parent()
        .map(|p| normalize_path(&p.to_string_lossy()))
    else {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!("cannot determine the folder of `{canonical}`"),
        ));
    };

    let reclaimed_bytes = compute_path_allocated_bytes(&target);
    bin_verdict(&bin_facts(&canonical), reclaimed_bytes).map_err(|why| {
        SamoError::new(
            ErrorCode::ProtectedPathDenied,
            format!("Samo won't move `{canonical}`: {why}. Nothing was changed."),
        )
    })?;

    let started_secs = chrono::Utc::now().timestamp().saturating_sub(2);
    let deleting = target.clone();
    on_own_thread(move || trash::delete(&deleting))?;

    // Found again by identity, not by the name the Recycle Bin shows (which hides known extensions): a recycle on
    // the same drive keeps the file's index, and an older item from the same folder never matches.
    let item = on_own_thread(trash::os_limited::list)?
        .into_iter()
        .filter(|item| {
            item.time_deleted >= started_secs
                && normalize_path(&item.original_parent.to_string_lossy())
                    .eq_ignore_ascii_case(&parent)
        })
        .find(|item| inspect_node_identity(Path::new(&item.id)) == Some(scanned_identity))
        .ok_or_else(|| {
            SamoError::new(
                ErrorCode::InternalError,
                format!(
                    "Samo moved `{canonical}` but can't find it in the Recycle Bin; look for it there by name"
                ),
            )
        })?;

    Ok(TrashOutcome {
        original_path: canonical,
        trashed_path: item.id.to_string_lossy().into_owned(),
        reclaimed_bytes,
    })
}

/// Undo on Windows: puts the Recycle Bin item `trashed_path` back at `original_path`, if that is the folder it came
/// from and nothing at all is there now (not even a link that points nowhere).
pub fn restore_from_trash(trashed_path: &Path, original_path: &Path) -> SamoResult<()> {
    match fs::symlink_metadata(original_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        _ => {
            return Err(SamoError::new(
                ErrorCode::InvalidInput,
                format!(
                    "cannot restore: `{}` already exists",
                    original_path.display()
                ),
            ));
        }
    }
    let (Some(wanted_parent), Some(name)) = (
        original_path.parent().and_then(canonical_folder),
        original_path.file_name(),
    ) else {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!(
                "cannot restore to `{}`: its folder is gone",
                original_path.display()
            ),
        ));
    };
    let id = trashed_path.as_os_str().to_os_string();
    let mut item = on_own_thread(trash::os_limited::list)?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| {
            SamoError::new(
                ErrorCode::NotFound,
                format!(
                    "`{}` is no longer in the Recycle Bin",
                    trashed_path.display()
                ),
            )
        })?;
    let came_from = normalize_path(&item.original_parent.to_string_lossy());
    if !came_from.eq_ignore_ascii_case(&wanted_parent) {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!(
                "the Recycle Bin item came from `{came_from}`, not `{}`",
                original_path.display()
            ),
        ));
    }
    // The Recycle Bin shows names without known extensions; the item goes back under its real name.
    item.name = name.to_os_string();
    on_own_thread(move || trash::os_limited::restore_all([item]))
}

/// A folder's canonical path in the one shape, if it is there.
fn canonical_folder(folder: &Path) -> Option<String> {
    fs::canonicalize(folder)
        .ok()
        .map(|p| normalize_path(&p.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn what_samo_wont_recycle_is_known_before_anything_moves() {
        // Windows' own folder (R27), and a path that isn't there, are refused without touching the Recycle Bin.
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        assert!(can_recycle(Path::new(&windows)).is_err());
        assert!(can_recycle(Path::new(r"C:\no\such\folder\for\samo")).is_err());
    }

    /// Hygiene for tests on the real Recycle Bin: puts back anything that came from under `dir`, so a failed lookup
    /// can never leave a test's files in KJ's bin.
    fn put_back_strays(dir: &Path) {
        let Some(dir) = canonical_path(dir).map(|d| d.to_ascii_lowercase()) else {
            return;
        };
        if let Ok(items) = trash::os_limited::list() {
            let strays: Vec<_> = items
                .into_iter()
                .filter(|i| {
                    is_on(
                        &normalize_path(&i.original_parent.to_string_lossy()).to_ascii_lowercase(),
                        &dir,
                    )
                })
                .collect();
            let _ = trash::os_limited::restore_all(strays);
        }
    }

    /// A file whose extension Windows hides (`.mp4`) is found again by its identity, not by its shown name.
    #[test]
    fn recycles_and_restores_a_single_file_with_its_extension() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let file = tmp.path().join("movie.mp4");
        fs::write(&file, vec![7u8; 8192]).map_err(|e| e.to_string())?;
        let id = inspect_node_identity(&file).ok_or("no identity")?;

        let outcome = move_to_trash(&file, id, Path::new("unused"));
        let gone = !file.exists();
        let restored = outcome
            .as_ref()
            .map(|o| restore_from_trash(Path::new(&o.trashed_path), &file));
        put_back_strays(tmp.path());

        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(gone);
        assert!(matches!(restored, Ok(Ok(()))), "{restored:?}");
        assert_eq!(inspect_node_identity(&file), Some(id));
        Ok(())
    }

    /// R2 on Windows decides before anything moves: only a fixed NTFS volume Samo can see, whose Recycle Bin is on
    /// and can hold the item (ADR-006 R27). Anything Samo can't tell is a no.
    #[test]
    fn the_bin_rules_refuse_before_anything_moves() {
        let ok = BinFacts {
            listed: true,
            removable: false,
            file_system: "NTFS".to_string(),
            nuke_on_delete: Some(0),
            max_capacity_mb: Some(7047),
        };
        let gb = 1024 * 1024 * 1024;
        assert!(bin_verdict(&ok, gb).is_ok());
        assert!(bin_verdict(&ok, 8 * gb).is_err(), "too big for the bin");
        let refused = [
            BinFacts {
                nuke_on_delete: Some(1),
                ..ok.clone()
            },
            BinFacts {
                nuke_on_delete: None,
                ..ok.clone()
            },
            BinFacts {
                max_capacity_mb: None,
                ..ok.clone()
            },
            BinFacts {
                removable: true,
                ..ok.clone()
            },
            BinFacts {
                listed: false,
                ..ok.clone()
            },
            BinFacts {
                file_system: "FAT32".to_string(),
                ..ok.clone()
            },
            BinFacts {
                file_system: "ReFS".to_string(),
                ..ok.clone()
            },
        ];
        for facts in refused {
            assert!(bin_verdict(&facts, 1024).is_err(), "{facts:?}");
        }
    }

    /// Undo never lands on something at the original path, not even a link that points nowhere.
    #[test]
    fn restore_never_lands_on_a_link() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let link = tmp.path().join("target");
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(tmp.path().join("nowhere"))
            .output()
            .map(|o| o.status.success())
            .map_err(|e| e.to_string())?;
        assert!(made, "mklink /J needs no admin rights");
        assert_eq!(
            restore_from_trash(Path::new(r"C:\no-such-bin-item"), &link).map_err(|e| e.code),
            Err(ErrorCode::InvalidInput)
        );
        Ok(())
    }

    /// The real Recycle Bin: the test always restores what it moved, before asserting anything else.
    #[test]
    fn recycles_and_restores_the_same_item() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        // As the temp folder reports itself, which may hold an 8.3 short name (`KONGPH~1`).
        let target = tmp.path().join("proj").join("target");
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        fs::write(target.join("a.rlib"), vec![1u8; 4096]).map_err(|e| e.to_string())?;
        let id = inspect_node_identity(&target).ok_or("no identity")?;

        let moved = move_to_trash(&target, id, Path::new("unused"));
        let gone = !target.exists();
        let restored = moved
            .as_ref()
            .map(|o| restore_from_trash(Path::new(&o.trashed_path), &target))
            .map_err(|e| e.code);
        put_back_strays(tmp.path());
        let outcome = moved.map_err(|e| e.message)?;

        assert!(matches!(restored, Ok(Ok(()))), "{restored:?}");
        assert!(gone, "the target should have left its folder");
        assert!(!outcome.trashed_path.is_empty());
        assert!(outcome.reclaimed_bytes > 0);
        assert!(
            !outcome.original_path.contains('~'),
            "{}",
            outcome.original_path
        );
        assert_eq!(inspect_node_identity(&target), Some(id));
        assert_eq!(
            restore_from_trash(Path::new(&outcome.trashed_path), &target).map_err(|e| e.code),
            Err(ErrorCode::InvalidInput)
        );
        Ok(())
    }

    #[test]
    fn a_replaced_target_is_refused_and_untouched() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let target = tmp.path().join("proj").join("target");
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        let scanned = inspect_node_identity(&target).ok_or("no identity")?;
        // Replaced under the same name since the scan.
        fs::remove_dir(&target).map_err(|e| e.to_string())?;
        fs::create_dir(&target).map_err(|e| e.to_string())?;
        fs::write(target.join("new.rlib"), b"keep me").map_err(|e| e.to_string())?;

        assert_eq!(
            move_to_trash(&target, scanned, Path::new("unused")).map_err(|e| e.code),
            Err(ErrorCode::InodeIdentityMismatch)
        );
        assert!(target.join("new.rlib").exists());
        assert_eq!(
            move_to_trash(Path::new(r"C:\Windows"), scanned, Path::new("unused"))
                .map_err(|e| e.code),
            Err(ErrorCode::ProtectedPathDenied)
        );
        Ok(())
    }
}
