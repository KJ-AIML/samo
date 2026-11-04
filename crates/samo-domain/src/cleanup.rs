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

/// Windows folders that hold the users' homes, each of which is protected (ADR-006 R27), lower-cased.
/// `Documents and Settings` is Windows' own link to `Users`.
const WINDOWS_HOME_PARENTS: &[&str] = &["users", "documents and settings"];

/// Windows names protected at a drive's root (ADR-006 R27), lower-cased.
const WINDOWS_PROTECTED_AT_ROOT: &[&str] = &[
    "system volume information",
    "recovery",
    "boot",
    "bootmgr",
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
];

/// A name Windows resolves to somewhere else, so it can't be judged by how it reads: `.` and `..`, a trailing dot
/// or space (Windows drops them), a stream (`name:stream`) and an 8.3 short name (`PROGRA~1`).
fn is_windows_alias(segment: &str) -> bool {
    segment.ends_with('.')
        || segment.ends_with(' ')
        || segment.contains(':')
        || segment
            .as_bytes()
            .windows(2)
            .any(|w| w[0] == b'~' && w[1].is_ascii_digit())
}

/// R2 on Windows, for the part of a `C:/…` path after the drive. NTFS ignores case, so this does too.
fn is_protected_windows_path(after_drive: &str) -> bool {
    let lower = after_drive.to_ascii_lowercase();
    let segments: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
    let Some(first) = segments.first().copied() else {
        return true; // the drive root
    };
    segments.iter().any(|s| is_windows_alias(s))
        || WINDOWS_PROTECTED_TREES.contains(&first)
        || (WINDOWS_HOME_PARENTS.contains(&first) && segments.len() <= 2)
        || segments.contains(&"$recycle.bin")
        || (segments.len() == 1 && WINDOWS_PROTECTED_AT_ROOT.contains(&first))
}

/// R2 for the OS Samo runs on (ADR-006 R25–R27). On Windows only a drive path in the one shape can be trashed, and
/// by Windows' rules; a `/`-rooted path there is on the current drive, so it fails closed. On the Mac a drive path
/// is a relative name and fails closed, as it always did.
pub fn is_protected_path_on(raw_path: &str, windows: bool) -> bool {
    match (crate::paths::drive_rest(raw_path), windows) {
        (Some(after_drive), true) => is_protected_windows_path(after_drive),
        (None, true) | (Some(_), false) => true,
        (None, false) => is_protected_mac_path(raw_path),
    }
}

/// R2 for the OS this build runs on.
pub fn is_protected_system_path(raw_path: &str) -> bool {
    is_protected_path_on(raw_path, cfg!(windows))
}

/// R2 on the Mac (ADR-001), unchanged but for `//`: the Mac resolves `//System` to `/System` (ADR-006 R25).
fn is_protected_mac_path(raw_path: &str) -> bool {
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

    /// R2 by the Mac's rules, so it runs where they apply. On Windows a `/`-rooted path fails closed, and
    /// `a_drive_trash_target_follows_this_os` checks the identity instead (ADR-006 R25).
    #[cfg(not(windows))]
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

    /// ADR-006 R27: R2 by the rules Samo uses on Windows, in any case, failing closed on anything not in the one
    /// shape. Tested through `is_protected_path_on`, so the Windows rules run on every OS's CI.
    #[test]
    fn windows_system_paths_are_protected_in_any_case() {
        let on_windows = |p: &str| is_protected_path_on(p, true);
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
            "C:/Documents and Settings",
            "C:/Documents and Settings/kj",
            "C:/Boot",
            "C:/bootmgr",
            // On Windows a `/`-rooted path is on the current drive: `/Windows/System32` is `C:/Windows/System32`.
            "/Windows/System32",
            "/Users/kj/dev/app/target",
        ] {
            assert!(on_windows(p), "{p} should be protected");
        }
        for p in [
            "C:/Users/kj/Developer/app/target",
            "D:/Work/node_modules",
            "C:/Users/kj/AppData/Local/npm-cache",
            "C:/Users/kj/.cache",
            "C:/Users/kj/my.project/node_modules",
            "C:/Users/kj/a~b",
        ] {
            assert!(!on_windows(p), "{p} should be allowed");
        }
    }

    /// ADR-006 R27: names Windows resolves to somewhere else are never taken at face value: `.` and `..`, a
    /// trailing dot or space (Windows drops them), an 8.3 short name (`PROGRA~1`) and a stream (`name:stream`).
    #[test]
    fn windows_lookalike_forms_fail_closed() {
        for p in [
            "C:/Users/kj/..",
            "C:/Users/kj/.",
            "C:/./Windows",
            "C:/x/../Windows",
            "C:/Users/./kj",
            "C:/Windows.",
            "C:/Windows ",
            "C:/Windows./System32",
            "C:/Program Files ",
            "C:/Users./kj",
            "C:/$Recycle.Bin.",
            "C:/pagefile.sys.",
            "C:/PROGRA~1",
            "C:/PROGRA~1/Git",
            "C:/PROGRA~3",
            "C:/Users/KONGPH~1/dev/app/target",
            "C:/Program Files::$INDEX_ALLOCATION",
            "C:/Windows:stream",
        ] {
            assert!(is_protected_path_on(p, true), "{p} should be protected");
        }
    }

    /// ADR-006 R25: on the Mac a drive path is a relative name and stays protected, exactly as before; a
    /// `//`-prefixed path resolves to `/…` there, so `//System/Library` is protected like `/System/Library`.
    #[test]
    fn on_the_mac_drive_paths_stay_protected() {
        assert!(is_protected_path_on("C:/Users/kj/dev/app/target", false));
        assert!(is_protected_path_on("C:/Windows", false));
        assert!(!is_protected_path_on("/Users/kj/repo/target", false));
        assert!(is_protected_path_on("//System/Library", false));
        assert!(is_protected_path_on("//server/share/x", false));
    }

    /// On this OS's own rules, a drive path is checked for identity on Windows and refused as protected elsewhere.
    #[test]
    fn a_drive_trash_target_follows_this_os() {
        let id = FileNodeIdentity {
            device_id: 7,
            inode: 42,
        };
        let other = FileNodeIdentity {
            device_id: 7,
            inode: 43,
        };
        let target = "C:/Users/kj/dev/app/target";
        if cfg!(windows) {
            assert!(validate_trash_target(target, id, Some(id)).is_ok());
            assert_eq!(
                validate_trash_target(target, id, Some(other)).map_err(|e| e.code),
                Err(ErrorCode::InodeIdentityMismatch)
            );
        } else {
            assert_eq!(
                validate_trash_target(target, id, Some(id)).map_err(|e| e.code),
                Err(ErrorCode::ProtectedPathDenied)
            );
        }
    }
}
