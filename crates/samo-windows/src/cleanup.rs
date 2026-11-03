//! R2 on Windows (ADR-006 R27): cleanup goes to the Recycle Bin, never a permanent delete, and Undo brings back the
//! same item.

use crate::identity::{canonical_path, inspect_node_identity};
use crate::scanner::allocated_file_bytes;
use samo_domain::{normalize_path, validate_trash_target};
use samo_primitives::{ErrorCode, FileNodeIdentity, SamoError, SamoResult};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
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

/// A removable drive has no Recycle Bin: Windows would delete for good, so Samo won't clean there.
fn on_removable_drive(path: &str) -> bool {
    Disks::new_with_refreshed_list().list().iter().any(|disk| {
        disk.is_removable() && is_on(path, &normalize_path(&disk.mount_point().to_string_lossy()))
    })
}

/// A Recycle Bin call, on a thread of its own: COM starts fresh there whatever the caller's thread uses, and a
/// panic inside the `trash` crate comes back as an error rather than taking Samo down.
fn on_own_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, trash::Error> + Send + 'static,
) -> SamoResult<T> {
    std::thread::spawn(work)
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
/// the scan, then moves it to its drive's Recycle Bin. Never deletes for good: a removable drive is refused, and an
/// item Windows didn't keep in the Recycle Bin is reported. `_trash_dir` is the Mac's; Windows has a bin per drive.
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
    if on_removable_drive(&canonical) {
        return Err(SamoError::new(
            ErrorCode::ProtectedPathDenied,
            format!(
                "`{canonical}` is on a removable drive, which has no Recycle Bin; Samo never deletes for good"
            ),
        ));
    }
    let (Some(parent), Some(name)) = (target.parent(), target.file_name()) else {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!("cannot determine file name for `{canonical}`"),
        ));
    };
    let parent = normalize_path(&parent.to_string_lossy());
    let name = name.to_os_string();

    let reclaimed_bytes = compute_path_allocated_bytes(&target);
    let deleting = target.clone();
    on_own_thread(move || trash::delete(&deleting))?;

    // Find it in the Recycle Bin, so Undo can bring it back, and so a delete Windows didn't recycle is said.
    let item = on_own_thread(trash::os_limited::list)?
        .into_iter()
        .filter(|item| {
            item.name == name
                && normalize_path(&item.original_parent.to_string_lossy())
                    .eq_ignore_ascii_case(&parent)
        })
        .max_by_key(|item| item.time_deleted)
        .ok_or_else(|| {
            SamoError::new(
                ErrorCode::InternalError,
                format!("Windows didn't keep `{canonical}` in the Recycle Bin"),
            )
        })?;

    Ok(TrashOutcome {
        original_path: canonical,
        trashed_path: item.id.to_string_lossy().into_owned(),
        reclaimed_bytes,
    })
}

/// Undo on Windows: puts the Recycle Bin item `trashed_path` back where it was, if that is `original_path` and
/// nothing has taken its place.
pub fn restore_from_trash(trashed_path: &Path, original_path: &Path) -> SamoResult<()> {
    if original_path.exists() {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!(
                "cannot restore: `{}` already exists",
                original_path.display()
            ),
        ));
    }
    let id = trashed_path.as_os_str().to_os_string();
    let item = on_own_thread(trash::os_limited::list)?
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
    let was = normalize_path(&item.original_path().to_string_lossy()).to_ascii_lowercase();
    let wanted = canonical_path(original_path)
        .unwrap_or_else(|| normalize_path(&original_path.to_string_lossy()))
        .to_ascii_lowercase();
    if was != wanted {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!(
                "the Recycle Bin item came from `{was}`, not `{}`",
                original_path.display()
            ),
        ));
    }
    on_own_thread(move || trash::os_limited::restore_all([item]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// The real Recycle Bin: the test always restores what it moved, before asserting anything else.
    #[test]
    fn recycles_and_restores_the_same_item() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        // As the temp folder reports itself, which may hold an 8.3 short name (`KONGPH~1`).
        let target = tmp.path().join("proj").join("target");
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        fs::write(target.join("a.rlib"), vec![1u8; 4096]).map_err(|e| e.to_string())?;
        let id = inspect_node_identity(&target).ok_or("no identity")?;

        let outcome = move_to_trash(&target, id, Path::new("unused")).map_err(|e| e.message)?;
        let gone = !target.exists();
        let restored = restore_from_trash(Path::new(&outcome.trashed_path), &target);

        assert!(restored.is_ok(), "{restored:?}");
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
