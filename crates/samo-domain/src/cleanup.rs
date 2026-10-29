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

pub fn is_protected_system_path(raw_path: &str) -> bool {
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
}
