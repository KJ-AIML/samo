use samo_primitives::{ErrorCode, FileNodeIdentity, SamoError, SamoResult};

const PROTECTED_EXACT_PATHS: &[&str] = &[
    "/",
    "/System",
    "/Library",
    "/Applications",
    "/Users",
    "/Volumes",
    "/bin",
    "/sbin",
    "/usr",
    "/var",
    "/private",
    "/dev",
    "/etc",
    "/tmp",
    "/cores",
];

const PROTECTED_PREFIXES: &[&str] = &[
    "/System/",
    "/Library/",
    "/bin/",
    "/sbin/",
    "/usr/",
    "/private/",
    "/dev/",
];

/// Windows folders whose whole tree is protected (ADR-006 R27), lower-cased.
const WINDOWS_PROTECTED_TREES: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
];

/// Windows names protected at a drive's root (ADR-006 R27), lower-cased.
const WINDOWS_PROTECTED_AT_ROOT: &[&str] = &[
    "system volume information",
    "recovery",
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
];

/// R2 on Windows, for the part of a `C:/…` path after the drive. NTFS ignores case, so this does too.
fn is_protected_windows_path(after_drive: &str) -> bool {
    let lower = after_drive.to_ascii_lowercase();
    let segments: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
    let Some(first) = segments.first().copied() else {
        return true; // the drive root
    };
    WINDOWS_PROTECTED_TREES.contains(&first)
        || (first == "users" && segments.len() <= 2)
        || segments.contains(&"$recycle.bin")
        || (segments.len() == 1 && WINDOWS_PROTECTED_AT_ROOT.contains(&first))
}

pub fn is_protected_system_path(raw_path: &str) -> bool {
    if let Some(after_drive) = crate::paths::drive_rest(raw_path) {
        return is_protected_windows_path(after_drive);
    }
    // A UNC path (`//server/share`) is in neither shape Samo keeps (ADR-006 R26): fail closed.
    if raw_path.starts_with("//") {
        return true;
    }
    let trimmed = if raw_path.len() > 1 {
        raw_path.trim_end_matches('/')
    } else {
        raw_path
    };
    if trimmed.is_empty() || !trimmed.starts_with('/') {
        return true;
    }
    if PROTECTED_EXACT_PATHS.contains(&trimmed) {
        return true;
    }
    if PROTECTED_PREFIXES.iter().any(|p| trimmed.starts_with(p)) {
        return true;
    }
    // Reject deleting a user's home root directory directly (e.g. `/Users/kj` or `/.Trash`)
    let segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() == 2 && segments[0] == "Users" {
        return true;
    }
    if segments.last().copied() == Some(".Trash") || trimmed.contains("/.Trash/") {
        return true;
    }
    false
}

/// R2: Validates that a cleanup target is not a protected system/home root and that its
/// live `(device_id, inode)` identity matches the scanned node before moving to `~/.Trash`.
pub fn validate_trash_target(
    path: &str,
    scanned_identity: FileNodeIdentity,
    live_identity: Option<FileNodeIdentity>,
) -> SamoResult<()> {
    if is_protected_system_path(path) {
        return Err(SamoError::new(
            ErrorCode::ProtectedPathDenied,
            format!("R2 guard blocked trashing protected path `{path}`"),
        ));
    }

    let Some(live) = live_identity else {
        return Err(SamoError::new(
            ErrorCode::NotFound,
            format!("cleanup target `{path}` no longer exists"),
        ));
    };

    if scanned_identity.device_id != live.device_id || scanned_identity.inode != live.inode {
        return Err(SamoError::new(
            ErrorCode::InodeIdentityMismatch,
            format!(
                "R2 guard rejected `{path}`: scanned inode ({}:{}) != live inode ({}:{})",
                scanned_identity.device_id, scanned_identity.inode, live.device_id, live.inode
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_paths_and_inode_mismatch_rejected() {
        let id = FileNodeIdentity {
            device_id: 1,
            inode: 42,
        };
        assert!(validate_trash_target("/", id, Some(id)).is_err());
        assert!(validate_trash_target("/System/Library", id, Some(id)).is_err());
        assert!(validate_trash_target("/Users/kj", id, Some(id)).is_err());
        assert!(validate_trash_target("/Users/kj/.Trash", id, Some(id)).is_err());

        let mismatched = FileNodeIdentity {
            device_id: 1,
            inode: 99,
        };
        let err = validate_trash_target("/Users/kj/repo/target", id, Some(mismatched));
        assert_eq!(
            err.map_err(|e| e.code),
            Err(ErrorCode::InodeIdentityMismatch)
        );

        assert!(validate_trash_target("/Users/kj/repo/target", id, Some(id)).is_ok());
    }

    /// ADR-006 R27: R2 on Windows, in any case, and failing closed on any path not in the one shape.
    #[test]
    fn windows_system_paths_are_protected_in_any_case() {
        for p in [
            "C:/",
            "c:/",
            "C:/Windows",
            "C:/windows/System32",
            "C:/Program Files",
            "c:/program files/git",
            "C:/Program Files (x86)/Steam",
            "C:/ProgramData",
            "C:/ProgramData/x",
            "C:/Users",
            "C:/Users/kj",
            "C:/users/KJ/",
            "C:/$Recycle.Bin",
            "D:/$RECYCLE.BIN/S-1-5-21",
            "C:/System Volume Information",
            "C:/Recovery",
            "C:/pagefile.sys",
            "C:/hiberfil.sys",
            "C:/swapfile.sys",
            r"C:\Users\kj\repo\target",
            r"C:/Users\kj",
            "//server/share/x",
            r"\\server\share\x",
        ] {
            assert!(is_protected_system_path(p), "{p} should be protected");
        }
        for p in [
            "C:/Users/kj/Developer/app/target",
            "D:/Work/node_modules",
            "C:/Users/kj/AppData/Local/npm-cache",
        ] {
            assert!(!is_protected_system_path(p), "{p} should be allowed");
        }

        let id = FileNodeIdentity {
            device_id: 7,
            inode: 42,
        };
        let other = FileNodeIdentity {
            device_id: 7,
            inode: 43,
        };
        assert!(validate_trash_target("C:/Users/kj/dev/app/target", id, Some(id)).is_ok());
        assert_eq!(
            validate_trash_target("C:/Users/kj/dev/app/target", id, Some(other))
                .map_err(|e| e.code),
            Err(ErrorCode::InodeIdentityMismatch)
        );
    }
}
