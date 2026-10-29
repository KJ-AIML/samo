//! Where Samo can measure. Every Mac has the standard folders, so those are offered when they exist; any other
//! folder you pick is added and kept between launches (one path per line in
//! `~/Library/Application Support/Samo/locations.txt`). Nothing here assumes one person's folder layout.

use crate::theme::home_dir;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub name: String,
    pub icon: &'static str,
    pub path: String,
    /// Added by you (and so removable), not one of the standard folders.
    pub custom: bool,
}

fn store() -> Option<PathBuf> {
    let home = home_dir();
    (!home.is_empty())
        .then(|| PathBuf::from(home).join("Library/Application Support/Samo/locations.txt"))
}

/// The standard folders that exist on this Mac.
fn standard(home: &str, exists: impl Fn(&str) -> bool) -> Vec<Location> {
    let mut out = Vec::new();
    let mut add = |name: &str, icon: &'static str, path: String| {
        if exists(&path) {
            out.push(Location {
                name: name.to_string(),
                icon,
                path,
                custom: false,
            });
        }
    };
    if !home.is_empty() {
        add("Home", "home", home.to_string());
        add("Desktop", "desktop", format!("{home}/Desktop"));
        add("Documents", "document", format!("{home}/Documents"));
        add("Downloads", "download", format!("{home}/Downloads"));
        add("Developer", "folder", format!("{home}/Developer"));
    }
    add("Applications", "apps", "/Applications".to_string());
    out
}

/// Custom folders from the store's text: one path per line, blanks and duplicates dropped.
fn parse(text: &str, known: &[Location], exists: impl Fn(&str) -> bool) -> Vec<Location> {
    let mut out: Vec<Location> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let taken = known.iter().chain(out.iter()).any(|l| l.path == line);
        if taken || !exists(line) {
            continue;
        }
        let name = Path::new(line)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| line.to_string());
        out.push(Location {
            name,
            icon: "folder",
            path: line.to_string(),
            custom: true,
        });
    }
    out
}

pub fn load() -> Vec<Location> {
    let exists = |p: &str| Path::new(p).is_dir();
    let mut list = standard(&home_dir(), exists);
    if let Some(text) = store().and_then(|p| std::fs::read_to_string(p).ok()) {
        let custom = parse(&text, &list, exists);
        list.extend(custom);
    }
    list
}

fn save(list: &[Location]) -> std::io::Result<()> {
    let Some(path) = store() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = list
        .iter()
        .filter(|l| l.custom)
        .map(|l| format!("{}\n", l.path))
        .collect();
    std::fs::write(path, text)
}

/// Adds a folder you picked (if it isn't there already) and keeps it.
pub fn add(list: &mut Vec<Location>, path: &str) -> std::io::Result<()> {
    if list.iter().any(|l| l.path == path) {
        return Ok(());
    }
    list.extend(parse(path, list, |p| Path::new(p).is_dir()));
    save(list)
}

/// Forgets a folder you added. Standard folders stay.
pub fn remove(list: &mut Vec<Location>, path: &str) -> std::io::Result<()> {
    list.retain(|l| !(l.custom && l.path == path));
    save(list)
}

/// The name to show for a folder: a known location's, or the folder's own.
pub fn name_for(list: &[Location], path: &str) -> String {
    list.iter()
        .find(|l| l.path == path)
        .map(|l| l.name.clone())
        .unwrap_or_else(|| {
            Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Macintosh HD".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_folders_are_offered_only_when_they_exist() {
        let list = standard("/Users/a", |p| p != "/Users/a/Developer");
        let names: Vec<&str> = list.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Home", "Desktop", "Documents", "Downloads", "Applications"]
        );
        assert!(list.iter().all(|l| !l.custom));
    }

    #[test]
    fn custom_folders_skip_blanks_duplicates_and_missing_ones() {
        let known = standard("/Users/a", |_| true);
        let text = "/Volumes/Work/x\n\n/Users/a/Desktop\n/Volumes/Work/x\n/gone\n";
        let custom = parse(text, &known, |p| p != "/gone");
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].name, "x");
        assert!(custom[0].custom);
    }

    #[test]
    fn names_come_from_the_list_or_the_folder() {
        let list = standard("/Users/a", |_| true);
        assert_eq!(name_for(&list, "/Users/a"), "Home");
        assert_eq!(name_for(&list, "/Users/a/Developer/samo"), "samo");
        assert_eq!(name_for(&list, "/"), "Macintosh HD");
    }
}
