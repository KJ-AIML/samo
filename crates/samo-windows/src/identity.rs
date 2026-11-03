//! R2 identity on Windows (ADR-006 §3): `(volume serial, file index)` in place of `(device, inode)`.

use samo_domain::normalize_path;
use samo_primitives::FileNodeIdentity;
use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

/// `FILE_FLAG_BACKUP_SEMANTICS`: lets the handle be a folder's.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// `FILE_FLAG_OPEN_REPARSE_POINT`: a link is opened as itself, never what it points to (or a OneDrive download).
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

/// The identity of the file or folder at `path`, read through a handle that asks for no access to its contents.
pub fn inspect_node_identity(path: &Path) -> Option<FileNodeIdentity> {
    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let info = winapi_util::file::information(&file).ok()?;
    Some(FileNodeIdentity {
        device_id: info.volume_serial_number(),
        inode: info.file_index(),
    })
}

/// Where `path` really is, in the one shape: its folder resolved (long names, no `..`, true case) and its own name
/// kept, so a link is named as itself. A drive root resolves whole. `None` if the folder can't be resolved.
pub(crate) fn canonical_path(path: &Path) -> Option<String> {
    let resolved = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            fs::canonicalize(parent).ok()?.join(name)
        }
        _ => fs::canonicalize(path).ok()?,
    };
    Some(normalize_path(&resolved.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_domain::is_absolute_path;
    use std::fs;

    #[test]
    fn identity_is_stable_and_shared_by_hard_links() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let a = tmp.path().join("a.bin");
        let link = tmp.path().join("a_link.bin");
        let b = tmp.path().join("b.bin");
        fs::write(&a, b"x").map_err(|e| e.to_string())?;
        fs::hard_link(&a, &link).map_err(|e| e.to_string())?;
        fs::write(&b, b"y").map_err(|e| e.to_string())?;

        let ia = inspect_node_identity(&a);
        assert!(ia.is_some());
        assert_eq!(ia, inspect_node_identity(&a));
        assert_eq!(ia, inspect_node_identity(&link));
        assert_ne!(ia, inspect_node_identity(&b));
        assert!(inspect_node_identity(tmp.path()).is_some());
        assert_eq!(inspect_node_identity(&tmp.path().join("missing")), None);
        Ok(())
    }

    /// R2 is judged on the canonical long path: no `~1` short names, no `..`, one shape (ADR-006 R26, R27).
    #[test]
    fn canonical_path_is_long_and_in_the_one_shape() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let proj = tmp.path().join("proj");
        fs::create_dir_all(proj.join("target")).map_err(|e| e.to_string())?;

        let roundabout = proj.join("target").join("..").join("target");
        let canonical = canonical_path(&roundabout).ok_or("no canonical path")?;
        assert!(is_absolute_path(&canonical), "{canonical}");
        assert!(
            !canonical.contains('\\') && !canonical.contains("~1"),
            "{canonical}"
        );
        assert!(canonical.ends_with("/proj/target"), "{canonical}");
        assert_eq!(canonical_path(Path::new(r"C:\")).as_deref(), Some("C:/"));
        Ok(())
    }
}
