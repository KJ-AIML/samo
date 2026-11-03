//! The scanner on Windows: one path shape, identity per entry, links never followed (ADR-006).

use crate::identity::inspect_node_identity;
use samo_blueprint::ScanConfig;
use samo_domain::{
    RawDiskEntry, RawEntryKind, aggregate_scan_entries, detect_project_marker, normalize_path,
    validate_scan_config,
};
use samo_primitives::{ErrorCode, FileNodeIdentity, SamoError, SamoResult, ScanSnapshot};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::Disks;

/// An identity no real file has; an entry that carries it can never pass R2's identity check.
const NO_IDENTITY: FileNodeIdentity = FileNodeIdentity {
    device_id: 0,
    inode: 0,
};

/// The bytes a file takes on disk: a sparse or compressed file (a WSL or Docker disk image) takes less than its
/// length, and a OneDrive file that isn't downloaded takes none.
pub(crate) fn allocated_file_bytes(path: &Path, meta: &fs::Metadata) -> u64 {
    use filesize::PathExt;
    path.size_on_disk_fast(meta).unwrap_or_else(|_| meta.len())
}

/// Windows' own bookkeeping at a drive's root, never measured: the Recycle Bin and restore points.
fn is_skipped_at_drive_root(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    matches!(
        lower.get(1..),
        Some(":/$recycle.bin") | Some(":/system volume information")
    )
}

/// The volume holding `root` (in the one shape): its label, total, used and free bytes.
fn volume_stats(root: &str) -> (String, u64, u64, u64) {
    let root_lower = root.to_ascii_lowercase();
    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<(usize, String, u64, u64)> = None;
    for disk in disks.list() {
        let mount = normalize_path(&disk.mount_point().to_string_lossy());
        let mount_dir = format!("{}/", mount.trim_end_matches('/')).to_ascii_lowercase();
        let inside = root_lower == mount.to_ascii_lowercase() || root_lower.starts_with(&mount_dir);
        if inside && best.as_ref().is_none_or(|(len, ..)| mount.len() > *len) {
            let name = disk.name().to_string_lossy().into_owned();
            let label = if name.is_empty() {
                mount.trim_end_matches('/').to_string()
            } else {
                name
            };
            best = Some((
                mount.len(),
                label,
                disk.total_space(),
                disk.available_space(),
            ));
        }
    }
    match best {
        Some((_, label, total, free)) => (label, total, total.saturating_sub(free), free),
        None => (root.get(..2).unwrap_or(root).to_string(), 0, 0, 0),
    }
}

pub fn scan_directory(
    scan_id: String,
    root_path: &Path,
    raw_config: ScanConfig,
) -> SamoResult<ScanSnapshot> {
    let config = validate_scan_config(raw_config);
    let started_at_ms = chrono::Utc::now().timestamp_millis();

    let cannot_read = |e: std::io::Error| {
        SamoError::new(
            ErrorCode::NotFound,
            format!("cannot read scan root `{}`: {e}", root_path.display()),
        )
    };
    // Long names, true case and no `..`, so every path below is in the one shape (ADR-006 R26).
    let canonical_root = fs::canonicalize(root_path).map_err(cannot_read)?;
    if !fs::metadata(&canonical_root).map_err(cannot_read)?.is_dir() {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!("scan root `{}` is not a directory", root_path.display()),
        ));
    }
    let root_str = normalize_path(&canonical_root.to_string_lossy());

    let mut entries: Vec<RawDiskEntry> = Vec::new();
    let mut seen_files: HashSet<(u64, u64)> = HashSet::new();
    let mut unreadable_paths: u64 = 0;
    let mut stack: Vec<PathBuf> = vec![PathBuf::from(&root_str)];

    while let Some(current_dir) = stack.pop() {
        let dir_path_str = normalize_path(&current_dir.to_string_lossy());
        let dir_name = current_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir_path_str.clone());
        let parent_str = current_dir
            .parent()
            .map(|p| normalize_path(&p.to_string_lossy()));
        let dir_identity = inspect_node_identity(&current_dir).unwrap_or(NO_IDENTITY);

        let Ok(read_dir) = fs::read_dir(&current_dir) else {
            unreadable_paths = unreadable_paths.saturating_add(1);
            continue;
        };

        let mut detected_marker: Option<String> = None;
        let mut child_dirs: Vec<PathBuf> = Vec::new();

        for item_res in read_dir {
            let Ok(item) = item_res else {
                unreadable_paths = unreadable_paths.saturating_add(1);
                continue;
            };
            let child_path = item.path();
            let child_str = normalize_path(&child_path.to_string_lossy());
            if is_skipped_at_drive_root(&child_str) {
                continue;
            }
            // From the folder listing itself: a link (symlink or junction) is seen as a link, never followed.
            let Ok(meta) = item.metadata() else {
                unreadable_paths = unreadable_paths.saturating_add(1);
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }

            let child_name = item.file_name().to_string_lossy().into_owned();
            if detected_marker.is_none()
                && let Some(kind) = detect_project_marker(&child_name)
            {
                detected_marker = Some(kind.to_string());
            }

            if meta.is_dir() {
                child_dirs.push(child_path);
            } else if meta.is_file() {
                let Some(identity) = inspect_node_identity(&child_path) else {
                    unreadable_paths = unreadable_paths.saturating_add(1);
                    continue;
                };
                // Hard links are counted once (RF1).
                if !seen_files.insert((identity.device_id, identity.inode)) {
                    continue;
                }
                entries.push(RawDiskEntry {
                    path: child_str,
                    name: child_name,
                    parent_path: Some(dir_path_str.clone()),
                    kind: RawEntryKind::File,
                    allocated_bytes: allocated_file_bytes(&child_path, &meta),
                    identity,
                    project_marker: None,
                });
            }
        }

        entries.push(RawDiskEntry {
            path: dir_path_str,
            name: dir_name,
            parent_path: parent_str,
            kind: RawEntryKind::Directory,
            allocated_bytes: 0,
            identity: dir_identity,
            project_marker: detected_marker,
        });
        stack.extend(child_dirs);
    }

    let completed_at_ms = chrono::Utc::now().timestamp_millis();
    let (vol_name, vol_total, vol_used, vol_free) = volume_stats(&root_str);

    Ok(aggregate_scan_entries(
        scan_id,
        root_str,
        vol_name,
        started_at_ms,
        completed_at_ms,
        vol_total,
        vol_used,
        vol_free,
        unreadable_paths,
        &entries,
        config,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_domain::is_absolute_path;
    use std::process::Command;

    #[test]
    fn scans_a_project_in_one_windows_shape() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = tmp.path();
        let proj = root.join("rust-app");
        let target = proj.join("target");
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        fs::write(proj.join("Cargo.toml"), b"[package]\nname = \"demo\"\n")
            .map_err(|e| e.to_string())?;
        fs::write(target.join("app.bin"), vec![b'x'; 65_536]).map_err(|e| e.to_string())?;
        fs::hard_link(target.join("app.bin"), target.join("app_link.bin"))
            .map_err(|e| e.to_string())?;
        // A junction back to `target`: never followed (a test of R1 and of honest totals).
        let junction = root.join("link-to-target");
        let made_junction = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let snap = scan_directory("scn_w".to_string(), root, ScanConfig::default())
            .map_err(|e| e.message)?;

        assert_eq!(snap.scanned_files, 2, "Cargo.toml and app.bin, once each");
        assert_eq!(snap.projects.len(), 1);
        assert_eq!(snap.projects[0].name, "rust-app");
        assert!(snap.projects[0].artifact_bytes >= 65_536);
        assert!(snap.projects[0].artifact_bytes < 2 * 65_536);
        assert!(is_absolute_path(&snap.root_path), "{}", snap.root_path);
        assert!(!snap.root_path.contains('~'), "{}", snap.root_path);
        let paths = snap
            .largest_folders
            .iter()
            .map(|f| f.path.as_str())
            .chain(snap.largest_files.iter().map(|f| f.path.as_str()))
            .chain(snap.projects.iter().map(|p| p.root_path.as_str()))
            .chain(
                snap.projects
                    .iter()
                    .flat_map(|p| p.artifacts.iter().map(|a| a.path.as_str())),
            );
        for path in paths {
            assert!(!path.contains('\\') && !path.contains("~1"), "{path}");
        }
        if made_junction {
            assert!(
                !snap
                    .largest_folders
                    .iter()
                    .any(|f| f.path.ends_with("/link-to-target"))
            );
        }
        Ok(())
    }
}
