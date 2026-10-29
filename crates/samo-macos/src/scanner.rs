use samo_blueprint::ScanConfig;
use samo_domain::{
    RawDiskEntry, RawEntryKind, aggregate_scan_entries, detect_project_marker, validate_scan_config,
};
use samo_primitives::{ErrorCode, FileNodeIdentity, SamoError, SamoResult, ScanSnapshot};
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use sysinfo::Disks;

pub fn inspect_node_identity(path: &Path) -> Option<FileNodeIdentity> {
    let meta = fs::symlink_metadata(path).ok()?;
    Some(FileNodeIdentity {
        device_id: meta.dev(),
        inode: meta.ino(),
    })
}

fn allocated_file_bytes(meta: &fs::Metadata) -> u64 {
    let blocks = meta.blocks();
    if blocks > 0 {
        blocks.saturating_mul(512)
    } else {
        meta.len()
    }
}

fn should_skip_system_mirror(scan_root: &Path, candidate: &Path) -> bool {
    let candidate_str = candidate.to_string_lossy();
    if candidate_str == "/dev" || candidate_str.starts_with("/dev/") {
        return true;
    }
    // RF1: When scanning `/`, skip `/System/Volumes/Data` APFS firmlink mirror so `/Users` isn't counted twice
    if scan_root == Path::new("/")
        && (candidate_str == "/System/Volumes/Data"
            || candidate_str.starts_with("/System/Volumes/Data/"))
    {
        return true;
    }
    false
}

/// Sums an entire `.app` bundle directory atomically without emitting individual internal files.
fn sum_app_bundle_bytes(
    bundle_dir: &Path,
    seen_inodes: &mut HashSet<(u64, u64)>,
    unreadable_paths: &mut u64,
) -> u64 {
    let mut total: u64 = 0;
    let mut stack: Vec<PathBuf> = vec![bundle_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(read_dir) = fs::read_dir(&dir) else {
            *unreadable_paths = unreadable_paths.saturating_add(1);
            continue;
        };
        for entry_res in read_dir {
            let Ok(entry) = entry_res else {
                *unreadable_paths = unreadable_paths.saturating_add(1);
                continue;
            };
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                *unreadable_paths = unreadable_paths.saturating_add(1);
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() && seen_inodes.insert((meta.dev(), meta.ino())) {
                total = total.saturating_add(allocated_file_bytes(&meta));
            }
        }
    }
    total
}

fn volume_stats_for_path(root: &Path) -> (String, u64, u64, u64) {
    let disks = Disks::new_with_refreshed_list();
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut best_match: Option<(usize, String, u64, u64)> = None;

    for disk in disks.list() {
        let mount = disk.mount_point();
        if canonical.starts_with(mount) {
            let mount_len = mount.as_os_str().len();
            let should_take = best_match
                .as_ref()
                .map(|(best_len, _, _, _)| mount_len > *best_len)
                .unwrap_or(true);
            if should_take {
                let name = disk.name().to_string_lossy().into_owned();
                let label = if name.is_empty() {
                    "Macintosh HD".to_string()
                } else {
                    name
                };
                best_match = Some((mount_len, label, disk.total_space(), disk.available_space()));
            }
        }
    }

    if let Some((_, name, total, avail)) = best_match {
        let used = total.saturating_sub(avail);
        (name, total, used, avail)
    } else {
        ("Macintosh HD".to_string(), 0, 0, 0)
    }
}

pub fn scan_directory(
    scan_id: String,
    root_path: &Path,
    raw_config: ScanConfig,
) -> SamoResult<ScanSnapshot> {
    let config = validate_scan_config(raw_config);
    let started_at_ms = chrono::Utc::now().timestamp_millis();

    let root_meta = fs::symlink_metadata(root_path).map_err(|e| {
        SamoError::new(
            ErrorCode::NotFound,
            format!("cannot read scan root `{}`: {e}", root_path.display()),
        )
    })?;
    if !root_meta.is_dir() {
        return Err(SamoError::new(
            ErrorCode::InvalidInput,
            format!("scan root `{}` is not a directory", root_path.display()),
        ));
    }

    let root_str = if root_path == Path::new("/") {
        "/".to_string()
    } else {
        root_path
            .to_string_lossy()
            .trim_end_matches('/')
            .to_string()
    };

    let mut entries: Vec<RawDiskEntry> = Vec::new();
    let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
    let mut unreadable_paths: u64 = 0;

    let mut stack: Vec<PathBuf> = vec![PathBuf::from(&root_str)];

    while let Some(current_dir) = stack.pop() {
        if should_skip_system_mirror(Path::new(&root_str), &current_dir) {
            continue;
        }

        let Ok(dir_meta) = fs::symlink_metadata(&current_dir) else {
            unreadable_paths = unreadable_paths.saturating_add(1);
            continue;
        };
        if dir_meta.file_type().is_symlink() {
            continue;
        }

        let dir_path_str = if current_dir == Path::new("/") {
            "/".to_string()
        } else {
            current_dir
                .to_string_lossy()
                .trim_end_matches('/')
                .to_string()
        };
        let dir_name = current_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        let parent_str = current_dir.parent().map(|p| {
            let s = p.to_string_lossy().into_owned();
            if s.len() > 1 {
                s.trim_end_matches('/').to_string()
            } else {
                s
            }
        });

        // If this directory is a macOS `.app` bundle, roll it up atomically
        if dir_name.ends_with(".app") && current_dir != Path::new(&root_str) {
            let bundle_bytes =
                sum_app_bundle_bytes(&current_dir, &mut seen_inodes, &mut unreadable_paths);
            entries.push(RawDiskEntry {
                path: dir_path_str,
                name: dir_name,
                parent_path: parent_str,
                kind: RawEntryKind::AppBundle,
                allocated_bytes: bundle_bytes,
                identity: FileNodeIdentity {
                    device_id: dir_meta.dev(),
                    inode: dir_meta.ino(),
                },
                project_marker: None,
            });
            continue;
        }

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
            if should_skip_system_mirror(Path::new(&root_str), &child_path) {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(&child_path) else {
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
                // RF1: Deduplicate hardlinked files by (dev, ino)
                if !seen_inodes.insert((meta.dev(), meta.ino())) {
                    continue;
                }
                let child_path_str = child_path.to_string_lossy().into_owned();
                entries.push(RawDiskEntry {
                    path: child_path_str,
                    name: child_name,
                    parent_path: Some(dir_path_str.clone()),
                    kind: RawEntryKind::File,
                    allocated_bytes: allocated_file_bytes(&meta),
                    identity: FileNodeIdentity {
                        device_id: meta.dev(),
                        inode: meta.ino(),
                    },
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
            identity: FileNodeIdentity {
                device_id: dir_meta.dev(),
                inode: dir_meta.ino(),
            },
            project_marker: detected_marker,
        });

        for d in child_dirs {
            stack.push(d);
        }
    }

    let completed_at_ms = chrono::Utc::now().timestamp_millis();
    let (vol_name, vol_total, vol_used, vol_free) = volume_stats_for_path(root_path);

    let snapshot = aggregate_scan_entries(
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
    );
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn deduplicates_hardlinks_and_skips_apfs_firmlink_mirror() {
        let Ok(tmp) = tempfile::tempdir() else { return };
        let root = tmp.path();

        let proj = root.join("rust-app");
        let target_dir = proj.join("target");
        let _ = fs::create_dir_all(&target_dir);

        if let Ok(mut f) = File::create(proj.join("Cargo.toml")) {
            let _ = f.write_all(b"[package]\nname = \"demo\"\n");
        }

        let original_bin = target_dir.join("app.bin");
        if let Ok(mut f) = File::create(&original_bin) {
            let payload = vec![b'x'; 65_536];
            let _ = f.write_all(&payload);
        }
        // Create hardlink to the same inode
        let hardlinked_bin = target_dir.join("app_link.bin");
        let _ = fs::hard_link(&original_bin, &hardlinked_bin);

        let snap_res = scan_directory("scn_test".to_string(), root, ScanConfig::default());
        assert!(snap_res.is_ok());
        let Ok(snap) = snap_res else { return };

        // 3 files total, but hardlinked_bin shares (dev, ino) with original_bin so only 2 files counted
        assert_eq!(snap.scanned_files, 2);
        assert_eq!(snap.projects.len(), 1);
        assert_eq!(snap.projects[0].name, "rust-app");

        assert!(should_skip_system_mirror(
            Path::new("/"),
            Path::new("/System/Volumes/Data/Users/kj")
        ));
    }
}
