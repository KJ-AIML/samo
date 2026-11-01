use crate::{ExtRoot, InstallError};
use samo_domain::extensions::{ArchiveEntry, EntryKind, validate_entries};
use samo_ext_protocol::Manifest;
use std::fs;
use std::os::unix::fs::{MetadataExt, symlink};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The stamp of what was copied, so an unchanged dev folder isn't copied again at every start.
const STAMP: &str = ".source-stamp";

#[derive(Debug, Clone)]
pub struct DevInstall {
    pub manifest: Manifest,
    pub source: PathBuf,
    /// False when the copy already matched the source.
    pub copied: bool,
}

/// The extensions in a dev folder: the folder itself if it holds `extension.toml`, else each folder in it that
/// does, by name.
pub fn find_dev_packages(dev_dir: &Path) -> Vec<PathBuf> {
    if dev_dir.join("extension.toml").is_file() {
        return vec![dev_dir.to_path_buf()];
    }
    let mut found: Vec<PathBuf> = fs::read_dir(dev_dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("extension.toml").is_file())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// Copies an unpacked extension from a developer's folder into `<root>/<id>/<version>/` and points `current` at
/// it, exactly like a catalog install, so hooks and paths are the same. Every entry is checked first (R5).
pub fn install_dev(root: &ExtRoot, src: &Path) -> Result<DevInstall, InstallError> {
    let text = fs::read_to_string(src.join("extension.toml"))
        .map_err(|e| InstallError::io("couldn't read extension.toml", e))?;
    let manifest = Manifest::parse(&text).map_err(InstallError::Manifest)?;
    let id = manifest.id.clone();

    let mut entries = Vec::new();
    walk(src, &id, &mut entries)?;
    let checked: Vec<ArchiveEntry> = entries.iter().map(|(entry, _, _)| entry.clone()).collect();
    validate_entries(&id, &checked).map_err(InstallError::Package)?;
    let helper_entry = format!("{id}/{}", manifest.helper);
    if !checked
        .iter()
        .any(|e| e.kind == EntryKind::File && e.path == helper_entry)
    {
        return Err(InstallError::NoHelper(manifest.helper.clone()));
    }

    let stamp = stamp_of(&entries);
    let version_dir = root.version_dir(&id, &manifest.version);
    let unchanged = fs::read_to_string(version_dir.join(STAMP)).is_ok_and(|s| s == stamp);
    let ext_dir = root.ext_dir(&id);
    fs::create_dir_all(&ext_dir).map_err(|e| InstallError::io("couldn't create its folder", e))?;

    if !unchanged {
        let staging = ext_dir.join(format!(".staging-{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        let copied = copy_entries(&staging, &id, &entries)
            .and_then(|()| {
                fs::write(staging.join(STAMP), &stamp)
                    .map_err(|e| InstallError::io("couldn't write its stamp", e))
            })
            .and_then(|()| replace_dir(&staging, &version_dir));
        if let Err(e) = copied {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    }

    point_current(root, &id, &manifest.version)?;
    remove_other_versions(&ext_dir, &manifest.version);
    fs::create_dir_all(root.data_dir(&id))
        .map_err(|e| InstallError::io("couldn't create its data folder", e))?;

    Ok(DevInstall {
        manifest,
        source: src.to_path_buf(),
        copied: !unchanged,
    })
}

/// Every entry under `dir`, as the package would hold it (`<id>/…`), with its full path and modified time.
/// Links are reported as links and never followed.
fn walk(
    dir: &Path,
    id: &str,
    out: &mut Vec<(ArchiveEntry, PathBuf, u128)>,
) -> Result<(), InstallError> {
    fn visit(
        base: &Path,
        dir: &Path,
        id: &str,
        out: &mut Vec<(ArchiveEntry, PathBuf, u128)>,
    ) -> Result<(), InstallError> {
        let mut children: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| InstallError::io("couldn't read the dev folder", e))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        children.sort();
        for path in children {
            let meta = fs::symlink_metadata(&path)
                .map_err(|e| InstallError::io("couldn't read the dev folder", e))?;
            let rel = path
                .strip_prefix(base)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let kind = if meta.file_type().is_symlink() {
                EntryKind::Symlink
            } else if meta.is_dir() {
                EntryKind::Dir
            } else if meta.is_file() && meta.nlink() > 1 {
                EntryKind::Hardlink
            } else if meta.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            out.push((
                ArchiveEntry {
                    path: format!("{id}/{rel}"),
                    kind,
                    size: if kind == EntryKind::File {
                        meta.len()
                    } else {
                        0
                    },
                },
                path.clone(),
                modified,
            ));
            if kind == EntryKind::Dir {
                visit(base, &path, id, out)?;
            }
        }
        Ok(())
    }
    visit(dir, dir, id, out)
}

/// What the source looks like: every path with its size and modified time.
fn stamp_of(entries: &[(ArchiveEntry, PathBuf, u128)]) -> String {
    entries
        .iter()
        .map(|(e, _, modified)| format!("{} {} {modified}\n", e.path, e.size))
        .collect()
}

fn copy_entries(
    dest: &Path,
    id: &str,
    entries: &[(ArchiveEntry, PathBuf, u128)],
) -> Result<(), InstallError> {
    let fail = |e| InstallError::io("couldn't copy the dev folder", e);
    fs::create_dir_all(dest).map_err(fail)?;
    let prefix = format!("{id}/");
    for (entry, from, _) in entries {
        let rel = entry.path.strip_prefix(&prefix).unwrap_or(&entry.path);
        let to = dest.join(rel);
        match entry.kind {
            EntryKind::Dir => fs::create_dir_all(&to).map_err(fail)?,
            // `fs::copy` keeps the permissions, so the helper stays executable.
            EntryKind::File => {
                fs::copy(from, &to).map_err(fail)?;
            }
            // Refused by `validate_entries` before anything is copied.
            EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other => {}
        }
    }
    Ok(())
}

/// Puts `staging` where `target` is, moving any old copy out of the way first.
fn replace_dir(staging: &Path, target: &Path) -> Result<(), InstallError> {
    if target.exists() {
        let old = target.with_file_name(format!(
            ".old-{}-{}",
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&old);
        fs::rename(target, &old)
            .map_err(|e| InstallError::io("couldn't replace the old copy", e))?;
        let _ = fs::remove_dir_all(&old);
    }
    fs::rename(staging, target).map_err(|e| InstallError::io("couldn't put the copy in place", e))
}

/// Points `current` at `version` with a relative link, replacing the old link in one rename so `current` is
/// never missing.
fn point_current(root: &ExtRoot, id: &str, version: &str) -> Result<(), InstallError> {
    let current = root.current(id);
    let tmp = root.ext_dir(id).join(".current-tmp");
    let _ = fs::remove_file(&tmp);
    symlink(version, &tmp).map_err(|e| InstallError::io("couldn't link the current version", e))?;
    fs::rename(&tmp, &current).map_err(|e| InstallError::io("couldn't link the current version", e))
}

/// Removes version folders other than `keep` (and leftovers from interrupted installs).
fn remove_other_versions(ext_dir: &Path, keep: &str) {
    let Ok(entries) = fs::read_dir(ext_dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_dir && name != keep && name != "data" {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn make_dev(dir: &Path, version: &str) -> Result<PathBuf, String> {
        let src = dir.join("dev/hello");
        fs::create_dir_all(src.join("bin")).map_err(|e| e.to_string())?;
        fs::write(
            src.join("extension.toml"),
            format!(
                "id = \"hello\"\nname = \"Hello\"\nversion = \"{version}\"\nprotocol = 1\nmin_samo = \"0.1.0\"\n\
                 helper = \"bin/hello\"\nsummary = \"Says hello\"\n\n[[changes]]\nwhat = \"Nothing\"\n\
                 where = \"nowhere\"\nundo = \"nothing to undo\"\n"
            ),
        )
        .map_err(|e| e.to_string())?;
        let helper = src.join("bin/hello");
        fs::write(&helper, "#!/bin/sh\necho hi\n").map_err(|e| e.to_string())?;
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Ok(src)
    }

    #[test]
    fn dev_install_copies_and_points_current() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let src = make_dev(dir.path(), "0.1.0")?;
        let root = ExtRoot::at(dir.path().join("Extensions"));

        let done = install_dev(&root, &src).map_err(|e| e.to_string())?;
        assert!(done.copied);
        assert_eq!(done.manifest.id, "hello");

        let helper = root.helper("hello", "bin/hello");
        let meta = fs::metadata(&helper).map_err(|e| e.to_string())?;
        assert_eq!(meta.permissions().mode() & 0o111, 0o111);
        let link = fs::read_link(root.current("hello")).map_err(|e| e.to_string())?;
        assert_eq!(link, PathBuf::from("0.1.0"));
        assert!(root.data_dir("hello").is_dir());
        assert_eq!(
            find_dev_packages(&dir.path().join("dev")),
            vec![src.clone()]
        );
        assert_eq!(find_dev_packages(&src), vec![src]);
        Ok(())
    }

    #[test]
    fn reinstall_when_source_changes() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let src = make_dev(dir.path(), "0.1.0")?;
        let root = ExtRoot::at(dir.path().join("Extensions"));
        install_dev(&root, &src).map_err(|e| e.to_string())?;
        let again = install_dev(&root, &src).map_err(|e| e.to_string())?;
        assert!(!again.copied, "an unchanged source isn't copied again");

        fs::write(src.join("bin/hello"), "#!/bin/sh\necho hello again\n")
            .map_err(|e| e.to_string())?;
        let changed = install_dev(&root, &src).map_err(|e| e.to_string())?;
        assert!(changed.copied);
        let copy =
            fs::read_to_string(root.helper("hello", "bin/hello")).map_err(|e| e.to_string())?;
        assert!(copy.contains("hello again"));
        Ok(())
    }

    #[test]
    fn current_swap_never_leaves_a_missing_link() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = ExtRoot::at(dir.path().join("Extensions"));
        let src = make_dev(dir.path(), "0.1.0")?;
        install_dev(&root, &src).map_err(|e| e.to_string())?;
        fs::write(root.data_dir("hello").join("kept"), "x").map_err(|e| e.to_string())?;

        let src = make_dev(dir.path(), "0.2.0")?;
        install_dev(&root, &src).map_err(|e| e.to_string())?;
        let link = fs::read_link(root.current("hello")).map_err(|e| e.to_string())?;
        assert_eq!(link, PathBuf::from("0.2.0"));
        assert!(root.helper("hello", "bin/hello").is_file());
        // The old version is gone, data stays, and nothing temporary is left behind.
        assert!(!root.version_dir("hello", "0.1.0").exists());
        assert!(root.data_dir("hello").join("kept").is_file());
        let names: Vec<String> = fs::read_dir(root.ext_dir("hello"))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with('.'))
            .collect();
        assert!(names.is_empty(), "{names:?}");
        Ok(())
    }

    #[test]
    fn dev_install_refuses_links_and_missing_helper() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = ExtRoot::at(dir.path().join("Extensions"));
        let src = make_dev(dir.path(), "0.1.0")?;
        symlink("/etc/passwd", src.join("bin/sneaky")).map_err(|e| e.to_string())?;
        assert!(matches!(
            install_dev(&root, &src),
            Err(InstallError::Package(_))
        ));
        assert!(
            !root.ext_dir("hello").exists(),
            "nothing is copied when refused"
        );

        fs::remove_file(src.join("bin/sneaky")).map_err(|e| e.to_string())?;
        fs::remove_file(src.join("bin/hello")).map_err(|e| e.to_string())?;
        assert!(matches!(
            install_dev(&root, &src),
            Err(InstallError::NoHelper(_))
        ));
        Ok(())
    }
}
