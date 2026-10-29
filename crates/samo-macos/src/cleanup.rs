use crate::scanner::inspect_node_identity;
use samo_domain::validate_trash_target;
use samo_primitives::{ErrorCode, FileNodeIdentity, SamoError, SamoResult};
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashOutcome {
    pub original_path: String,
    pub trashed_path: String,
    pub reclaimed_bytes: u64,
}

fn compute_path_allocated_bytes(target: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(target) else {
        return 0;
    };
    if meta.is_file() {
        let blocks = meta.blocks();
        return if blocks > 0 {
            blocks.saturating_mul(512)
        } else {
            meta.len()
        };
    }
    let mut sum: u64 = 0;
    let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
    let mut stack = vec![target.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for entry_res in rd {
            let Ok(entry) = entry_res else { continue };
            let p = entry.path();
            let Ok(m) = fs::symlink_metadata(&p) else {
                continue;
            };
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_dir() {
                stack.push(p);
            } else if m.is_file() && seen_inodes.insert((m.dev(), m.ino())) {
                let blocks = m.blocks();
                let b = if blocks > 0 {
                    blocks.saturating_mul(512)
                } else {
                    m.len()
                };
                sum = sum.saturating_add(b);
            }
        }
    }
    sum
}

fn next_available_trash_destination(trash_dir: &Path, base_name: &str) -> PathBuf {
    let initial = trash_dir.join(base_name);
    if !initial.exists() {
        return initial;
    }

    for index in 2..10_000 {
        let candidate_name = match base_name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => format!("{stem} {index}.{ext}"),
            _ => format!("{base_name} {index}"),
        };
        let candidate = trash_dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    trash_dir.join(format!(
        "{base_name} {}",
        chrono::Utc::now().timestamp_millis()
    ))
}

pub fn default_user_trash_dir() -> SamoResult<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| {
        SamoError::new(
            ErrorCode::InternalError,
            "HOME environment variable is not set",
        )
    })?;
    Ok(PathBuf::from(home).join(".Trash"))
}

/// R2 + RF2: Verifies `(device_id, inode)` identity and moves `target_path` into `trash_dir`
/// with Finder-style collision suffixing (`target`, `target 2`, `target 3`).
/// Never permanently deletes files.
pub fn move_to_trash(
    target_path: &Path,
    scanned_identity: FileNodeIdentity,
    trash_dir: &Path,
) -> SamoResult<TrashOutcome> {
    let path_str = target_path.to_string_lossy().into_owned();
    let live_identity = inspect_node_identity(target_path);

    validate_trash_target(&path_str, scanned_identity, live_identity)?;

    fs::create_dir_all(trash_dir).map_err(|e| {
        SamoError::new(
            ErrorCode::InternalError,
            format!(
                "failed to prepare Trash directory `{}`: {e}",
                trash_dir.display()
            ),
        )
    })?;

    let base_name = target_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| {
            SamoError::new(
                ErrorCode::InvalidInput,
                format!("cannot determine file name for `{path_str}`"),
            )
        })?;

    let reclaimed_bytes = compute_path_allocated_bytes(target_path);
    let destination = next_available_trash_destination(trash_dir, &base_name);

    fs::rename(target_path, &destination).map_err(|e| {
        SamoError::new(
            ErrorCode::InternalError,
            format!(
                "failed to move `{}` to Trash `{}`: {e}",
                target_path.display(),
                destination.display()
            ),
        )
    })?;

    Ok(TrashOutcome {
        original_path: path_str,
        trashed_path: destination.to_string_lossy().into_owned(),
        reclaimed_bytes,
    })
}

/// Restores a previously trashed item from `~/.Trash/...` back to its `original_path`.
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
    fs::rename(trashed_path, original_path).map_err(|e| {
        SamoError::new(
            ErrorCode::InternalError,
            format!(
                "failed to restore `{}` to `{}`: {e}",
                trashed_path.display(),
                original_path.display()
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn trashes_duplicate_target_names_without_collision() {
        let Ok(tmp) = tempfile::tempdir() else { return };
        let trash_dir = tmp.path().join(".Trash");
        let proj_a_target = tmp.path().join("proj-a").join("target");
        let proj_b_target = tmp.path().join("proj-b").join("target");

        let _ = fs::create_dir_all(&proj_a_target);
        let _ = fs::create_dir_all(&proj_b_target);

        if let Ok(mut f) = File::create(proj_a_target.join("a.rlib")) {
            let _ = f.write_all(&[1u8; 4096]);
        }
        if let Ok(mut f) = File::create(proj_b_target.join("b.rlib")) {
            let _ = f.write_all(&[2u8; 8192]);
        }

        let Some(id_a) = inspect_node_identity(&proj_a_target) else {
            return;
        };
        let Some(id_b) = inspect_node_identity(&proj_b_target) else {
            return;
        };

        let out_a = move_to_trash(&proj_a_target, id_a, &trash_dir);
        assert!(out_a.is_ok());
        let out_b = move_to_trash(&proj_b_target, id_b, &trash_dir);
        assert!(out_b.is_ok());

        if let (Ok(a), Ok(b)) = (out_a, out_b) {
            assert!(a.trashed_path.ends_with("/.Trash/target"));
            assert!(b.trashed_path.ends_with("/.Trash/target 2"));
            assert!(Path::new(&a.trashed_path).exists());
            assert!(Path::new(&b.trashed_path).exists());
        }
    }
}
