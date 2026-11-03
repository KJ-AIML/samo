//! ADR-006 R26: one shape for Windows paths, so the pure rules keep working on strings.
//!
//! A Windows path is kept as `C:/Users/kj/Developer`: `/` separators, an upper-case drive, and no trailing `/` except
//! on the drive root `C:/`. A `/`-rooted path is the Mac's and is never changed, since a Mac name may contain `\`.

/// The verbatim prefix Windows APIs may hand back (`\\?\C:\…`). Only the backslash form: anything starting with `/`
/// is never changed.
const VERBATIM: &str = r"\\?\";

/// `X:` followed by a separator, with X an ASCII letter.
fn has_drive(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/// Brings a Windows path to the one shape; leaves everything else, Mac paths included, as it is.
pub fn normalize_path(raw: &str) -> String {
    let path = raw.strip_prefix(VERBATIM).unwrap_or(raw);
    if !has_drive(path) {
        return raw.to_string();
    }
    let mut out: String = path
        .get(..1)
        .map(str::to_ascii_uppercase)
        .unwrap_or_default();
    out.push(':');
    let rest = path.get(2..).unwrap_or("").replace('\\', "/");
    let rest = rest.trim_end_matches('/');
    out.push('/');
    out.push_str(rest.trim_start_matches('/'));
    out
}

/// The part after `X:/` of a path in the one Windows shape; `None` for a Mac path or a path not normalised.
pub(crate) fn drive_rest(path: &str) -> Option<&str> {
    let b = path.as_bytes();
    let shaped = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/';
    if !shaped || path.contains('\\') {
        return None;
    }
    path.get(3..)
}

/// Absolute in one of the two shapes Samo keeps: `/…` on the Mac, `X:/…` on Windows. UNC paths aren't (yet).
pub fn is_absolute_path(path: &str) -> bool {
    (path.starts_with('/') && !path.starts_with("//")) || drive_rest(path).is_some()
}

/// The scan root to keep for `raw` on the OS Samo runs on (`windows`), or `None` if it can't be one. On Windows it
/// is a drive path in the one shape. On the Mac the rule is unchanged: any `/`-rooted path, as given (ADR-006 R25).
pub fn scan_root_on(raw: &str, windows: bool) -> Option<String> {
    if windows {
        let root = normalize_path(raw);
        drive_rest(&root).is_some().then_some(root)
    } else {
        raw.starts_with('/').then(|| raw.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_keeps_mac_paths_and_shapes_windows_ones() {
        assert_eq!(
            normalize_path(r"C:\Users\kj\Developer"),
            "C:/Users/kj/Developer"
        );
        assert_eq!(normalize_path("c:/users/kj/"), "C:/users/kj");
        assert_eq!(normalize_path(r"C:\"), "C:/");
        // Drive-relative: not a Windows absolute path, so it is left alone.
        assert_eq!(normalize_path("d:"), "d:");
        assert_eq!(normalize_path(r"\\?\D:\Work\app"), "D:/Work/app");
        // A Mac name may contain `\`: a `/`-rooted path is never changed.
        assert_eq!(normalize_path(r"/Users/kj/a\b"), r"/Users/kj/a\b");
        assert_eq!(normalize_path("/Users/kj/"), "/Users/kj/");
        assert_eq!(normalize_path("//?/C:/x"), "//?/C:/x");
        assert_eq!(normalize_path(r"relative\x"), r"relative\x");
    }

    /// ADR-006 R25/R26: on Windows a scan root is a drive path in the one shape; on the Mac the rule is unchanged.
    #[test]
    fn scan_roots_follow_the_os() {
        let windows = |r: &str| scan_root_on(r, true);
        let mac = |r: &str| scan_root_on(r, false);
        assert_eq!(
            windows(r"C:\Users\kj\Developer").as_deref(),
            Some("C:/Users/kj/Developer")
        );
        assert_eq!(windows("C:/").as_deref(), Some("C:/"));
        for no in [
            "/Users/kj",
            "//server/share",
            "Developer",
            "C:Developer",
            "",
        ] {
            assert_eq!(windows(no), None, "{no}");
        }
        assert_eq!(mac("/Users/kj/").as_deref(), Some("/Users/kj/"));
        assert_eq!(mac("//x").as_deref(), Some("//x"));
        for no in [r"C:\Users\kj", "C:/Users/kj", "Developer", ""] {
            assert_eq!(mac(no), None, "{no}");
        }
    }

    #[test]
    fn only_rooted_and_drive_paths_are_absolute() {
        for yes in ["/", "/Users/kj", "C:/", "C:/Users", "z:/x"] {
            assert!(is_absolute_path(yes), "{yes}");
        }
        for no in [
            "",
            "C:",
            "C:Users",
            r"C:\Users",
            "relative",
            "//server/share",
            "1:/x",
            "CC:/x",
        ] {
            assert!(!is_absolute_path(no), "{no}");
        }
    }

    #[test]
    fn drive_rest_is_what_follows_the_drive() {
        assert_eq!(drive_rest("C:/Users/kj"), Some("Users/kj"));
        assert_eq!(drive_rest("C:/"), Some(""));
        assert_eq!(drive_rest("/Users/kj"), None);
        assert_eq!(drive_rest(r"C:\Users"), None);
        // `\` is a separator on Windows: a half-normalised path is not in the one shape, so callers fail closed.
        assert_eq!(drive_rest(r"C:/Users\kj"), None);
        assert!(!is_absolute_path(r"C:/Users\kj"));
    }
}
