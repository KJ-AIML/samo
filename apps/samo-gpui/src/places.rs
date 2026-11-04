//! Where things are on each OS (ADR-006 §3): the home folder, the user's name, the name of a drive's root, and a
//! folder to measure in the one shape the scanner reports (`C:/Users/kj/…` on Windows), so a finished scan always
//! matches the folder it was asked for. The Mac's are exactly as they were.

use samo_domain::normalize_path;

/// The user's home folder: `HOME` on the Mac; on Windows, which has no `HOME`, the profile folder, in the one shape.
pub fn home_dir() -> String {
    if cfg!(windows) {
        std::env::home_dir()
            .map(|h| normalize_path(&h.to_string_lossy()))
            .unwrap_or_default()
    } else {
        std::env::var("HOME").unwrap_or_default()
    }
}

/// The user's name for the sidebar: `USER` on the Mac, `USERNAME` on Windows.
pub fn user_name() -> String {
    std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" })
        .unwrap_or_else(|_| "you".to_string())
}

/// The drive a Windows path starts on (`C:`), if it does.
fn drive_of(path: &str) -> Option<&str> {
    let b = path.as_bytes();
    (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        .then(|| path.get(..2))
        .flatten()
}

/// A root's name: a Windows drive by its letter (`C:`), the Mac's `/` as Macintosh HD.
pub fn root_name(root: &str) -> String {
    drive_of(root)
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| "Macintosh HD".to_string())
}

/// Breadcrumb segments for `root`: `(label, absolute path)`, with `home` shown as `~` and a drive by its letter.
pub fn path_segments_from(root: &str, home: &str) -> Vec<(String, String)> {
    let under_home = !home.is_empty() && (root == home || root.starts_with(&format!("{home}/")));
    let (mut acc, mut out, rest) = if under_home {
        (
            home.to_string(),
            vec![("~".to_string(), home.to_string())],
            root.get(home.len()..).unwrap_or("").to_string(),
        )
    } else if let Some(drive) = drive_of(root) {
        (
            drive.to_string(),
            vec![(drive.to_string(), format!("{drive}/"))],
            root.get(2..).unwrap_or("").to_string(),
        )
    } else {
        (
            String::new(),
            vec![("Macintosh HD".to_string(), "/".to_string())],
            root.to_string(),
        )
    };
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        acc = format!("{acc}/{part}");
        out.push((part.to_string(), acc.clone()));
    }
    out
}

/// `path` with `home` shortened to `~`.
pub fn display_path_from(path: &str, home: &str) -> String {
    if !home.is_empty() {
        if path == home {
            return "~".to_string();
        }
        if let Some(rest) = path.strip_prefix(&format!("{home}/")) {
            return format!("~/{rest}");
        }
    }
    path.to_string()
}

/// A folder to measure, in the shape the scanner reports it: on Windows its canonical path (long names, true case,
/// `C:/…`), so the finished scan matches the folder asked for; on the Mac the path as given.
pub fn scan_root(path: &str) -> String {
    if cfg!(windows) {
        std::fs::canonicalize(path)
            .map(|p| normalize_path(&p.to_string_lossy()))
            .unwrap_or_else(|_| normalize_path(path))
    } else {
        path.to_string()
    }
}

/// Shows `path` in Finder, with it selected (as before).
#[cfg(not(windows))]
pub fn reveal(path: &str) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status();
}

/// Shows `path` in Explorer, with it selected. Explorer is started and not waited for (it exits 1 even when it
/// worked), and gets `/select,"path"` exactly as it expects it.
#[cfg(windows)]
pub fn reveal(path: &str) {
    use std::os::windows::process::CommandExt;
    let explorer = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join("explorer.exe");
    let _ = std::process::Command::new(explorer)
        .raw_arg(format!("/select,\"{}\"", path.replace('/', "\\")))
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_paths_have_breadcrumbs_and_a_root() {
        assert_eq!(
            path_segments_from("D:/KJ/repo", "C:/Users/kj"),
            vec![
                ("D:".to_string(), "D:/".to_string()),
                ("KJ".to_string(), "D:/KJ".to_string()),
                ("repo".to_string(), "D:/KJ/repo".to_string()),
            ]
        );
        assert_eq!(
            path_segments_from("C:/Users/kj/dev", "C:/Users/kj"),
            vec![
                ("~".to_string(), "C:/Users/kj".to_string()),
                ("dev".to_string(), "C:/Users/kj/dev".to_string()),
            ]
        );
        assert_eq!(
            path_segments_from("/Applications", "/Users/kj"),
            vec![
                ("Macintosh HD".to_string(), "/".to_string()),
                ("Applications".to_string(), "/Applications".to_string()),
            ]
        );
        assert_eq!(root_name("C:/"), "C:");
        assert_eq!(root_name("/"), "Macintosh HD");
    }

    #[test]
    fn home_is_shown_as_tilde_on_both() {
        assert_eq!(display_path_from("C:/Users/kj/dev", "C:/Users/kj"), "~/dev");
        assert_eq!(display_path_from("C:/Users/kj", "C:/Users/kj"), "~");
        assert_eq!(display_path_from("/Users/kj/dev", "/Users/kj"), "~/dev");
        assert_eq!(display_path_from("D:/KJ", "C:/Users/kj"), "D:/KJ");
    }
}
