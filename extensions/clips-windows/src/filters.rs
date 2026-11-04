//! What Clips never keeps (ADR-005 R22), checked before anything copied is read. On Windows a password manager marks
//! its copies with registered clipboard formats; and copies made in an ignored app are left alone.

use std::collections::BTreeSet;

/// Present means "don't watch this copy" (Windows' own clipboard history honours it too).
pub const EXCLUDE_FROM_MONITORING: &str = "ExcludeClipboardContentFromMonitorProcessing";
/// Older clipboard tools' mark for the same.
pub const VIEWER_IGNORE: &str = "Clipboard Viewer Ignore";

/// What the watcher learned about a copy before reading it: its formats' names, and whether its DWORD markers say
/// "not in clipboard history" (`CanIncludeInClipboardHistory` = 0) or "not to the cloud" (`CanUploadToCloudClipboard`
/// = 0), which password managers set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    pub names: Vec<String>,
    pub no_history: bool,
    pub no_cloud: bool,
}

/// The password managers whose copies are never kept, by their program's name, in lowercase.
pub fn default_ignored_apps() -> BTreeSet<String> {
    [
        "1password.exe",
        "bitwarden.exe",
        "keepass.exe",
        "keepassxc.exe",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Whether a copy may be kept: no secret mark, and not made in an ignored app (`app` is the program in front).
pub fn allows(marks: &Marks, app: Option<&str>, ignored: &BTreeSet<String>) -> bool {
    let secret = marks
        .names
        .iter()
        .any(|n| n == EXCLUDE_FROM_MONITORING || n == VIEWER_IGNORE)
        || marks.no_history
        || marks.no_cloud;
    let from_ignored = app.is_some_and(|a| ignored.contains(&a.to_ascii_lowercase()));
    !secret && !from_ignored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(names: &[&str], no_history: bool, no_cloud: bool) -> Marks {
        Marks {
            names: names.iter().map(|n| n.to_string()).collect(),
            no_history,
            no_cloud,
        }
    }

    #[test]
    fn password_managers_markers_are_never_kept() {
        let ignored = default_ignored_apps();
        let plain = marks(&["CF_UNICODETEXT"], false, false);
        assert!(allows(&plain, Some("notepad.exe"), &ignored));
        assert!(!allows(
            &marks(&["CF_UNICODETEXT", EXCLUDE_FROM_MONITORING], false, false),
            None,
            &ignored
        ));
        assert!(!allows(
            &marks(&["CF_UNICODETEXT"], true, false),
            None,
            &ignored
        ));
        assert!(!allows(
            &marks(&["CF_UNICODETEXT"], false, true),
            None,
            &ignored
        ));
    }

    #[test]
    fn copies_in_ignored_apps_are_never_kept() {
        let ignored = default_ignored_apps();
        let plain = marks(&["CF_UNICODETEXT"], false, false);
        for app in [
            "1Password.exe",
            "bitwarden.exe",
            "KeePass.exe",
            "KeePassXC.exe",
        ] {
            assert!(!allows(&plain, Some(app), &ignored), "{app}");
        }
    }
}
