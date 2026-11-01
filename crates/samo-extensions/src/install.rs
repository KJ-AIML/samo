use crate::{ExtRoot, InstallError};
use flate2::read::GzDecoder;
use samo_domain::extensions::{
    ArchiveEntry, EntryKind, MAX_ENTRIES, MAX_UNPACKED_BYTES, validate_entries,
};
use samo_domain::store::{Listing, manifest_matches};
use samo_ext_protocol::Manifest;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
pub(crate) fn point_current(root: &ExtRoot, id: &str, version: &str) -> Result<(), InstallError> {
    let current = root.current(id);
    let tmp = root.ext_dir(id).join(".current-tmp");
    let _ = fs::remove_file(&tmp);
    symlink(version, &tmp).map_err(|e| InstallError::io("couldn't link the current version", e))?;
    fs::rename(&tmp, &current).map_err(|e| InstallError::io("couldn't link the current version", e))
}

/// Removes version folders other than `keep` (and leftovers from interrupted installs).
pub(crate) fn remove_other_versions(ext_dir: &Path, keep: &str) {
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

/// A store package unpacked into staging and checked: every entry (R5), the helper, and the manifest against its
/// catalog entry (R11). Nothing outside staging has changed yet.
#[derive(Debug)]
pub(crate) struct Staged {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

static STAGING: AtomicU64 = AtomicU64::new(0);

/// Unpacks a verified package (its hash already matched the signed catalog) for `listing` into staging beside
/// the extension's versions, and checks it.
pub(crate) fn stage_catalog(
    root: &ExtRoot,
    listing: &Listing,
    package: &[u8],
) -> Result<Staged, InstallError> {
    let id = &listing.id;
    let ext_dir = root.ext_dir(id);
    fs::create_dir_all(&ext_dir).map_err(|e| InstallError::io("couldn't create its folder", e))?;
    let n = STAGING.fetch_add(1, Ordering::Relaxed);
    let dir = ext_dir.join(format!(".staging-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let checked = unpack(package, id, &dir).and_then(|()| {
        let text = fs::read_to_string(dir.join("extension.toml"))
            .map_err(|e| InstallError::io("the package has no extension.toml", e))?;
        let manifest = Manifest::parse(&text).map_err(InstallError::Manifest)?;
        manifest_matches(&manifest, listing).map_err(InstallError::Store)?;
        if !dir.join(&manifest.helper).is_file() {
            return Err(InstallError::NoHelper(manifest.helper.clone()));
        }
        Ok(manifest)
    });
    match checked {
        Ok(manifest) => Ok(Staged { dir, manifest }),
        Err(e) => {
            let _ = fs::remove_dir_all(&dir);
            Err(e)
        }
    }
}

/// Where a version folder that was in the way went, so a failed install can put it back.
#[derive(Debug)]
pub(crate) struct Placed {
    pub target: PathBuf,
    pub aside: Option<PathBuf>,
}

impl Placed {
    /// The new version stays: what was set aside goes.
    pub fn keep(self) {
        if let Some(aside) = self.aside {
            let _ = fs::remove_dir_all(aside);
        }
    }

    /// The new version goes, and what was set aside comes back.
    pub fn undo(self) {
        let _ = fs::remove_dir_all(&self.target);
        if let Some(aside) = self.aside {
            let _ = fs::rename(aside, &self.target);
        }
    }
}

/// Renames staging into `<id>/<version>/`, first moving any folder already there aside (a dev build can have the
/// same version number). `current` is not touched.
pub(crate) fn place(root: &ExtRoot, staged: Staged) -> Result<Placed, InstallError> {
    let id = &staged.manifest.id;
    let target = root.version_dir(id, &staged.manifest.version);
    let aside = if target.exists() {
        let n = STAGING.fetch_add(1, Ordering::Relaxed);
        let aside = root
            .ext_dir(id)
            .join(format!(".aside-{}-{n}", std::process::id()));
        fs::rename(&target, &aside)
            .map_err(|e| InstallError::io("couldn't move the old copy aside", e))?;
        Some(aside)
    } else {
        None
    };
    if let Err(e) = fs::rename(&staged.dir, &target) {
        let _ = fs::remove_dir_all(&staged.dir);
        if let Some(aside) = &aside {
            let _ = fs::rename(aside, &target);
        }
        return Err(InstallError::io("couldn't put the new copy in place", e));
    }
    Ok(Placed { target, aside })
}

/// One tar header's fields that matter here.
struct Header {
    name: String,
    mode: u32,
    size: u64,
    kind: u8,
}

fn field(block: &[u8], from: usize, to: usize) -> &[u8] {
    let f = block.get(from..to).unwrap_or_default();
    let end = f.iter().position(|&b| b == 0).unwrap_or(f.len());
    f.get(..end).unwrap_or_default()
}

fn octal(block: &[u8], from: usize, to: usize) -> Option<u64> {
    let text = std::str::from_utf8(field(block, from, to)).ok()?.trim();
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(text, 8).ok()
}

fn header(block: &[u8; 512]) -> Result<Header, InstallError> {
    let bad = || InstallError::Io("the package isn't a valid archive".into());
    let sum: u64 = block
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            if (148..156).contains(&i) {
                32
            } else {
                u64::from(b)
            }
        })
        .sum();
    if octal(block, 148, 156) != Some(sum) {
        return Err(bad());
    }
    let mut name = String::from_utf8(field(block, 0, 100).to_vec()).map_err(|_| bad())?;
    if field(block, 257, 262) == b"ustar" {
        let prefix = String::from_utf8(field(block, 345, 500).to_vec()).map_err(|_| bad())?;
        if !prefix.is_empty() {
            name = format!("{prefix}/{name}");
        }
    }
    Ok(Header {
        name,
        mode: u32::try_from(octal(block, 100, 108).ok_or_else(bad)?).unwrap_or(0),
        size: octal(block, 124, 136).ok_or_else(bad)?,
        kind: block.get(156).copied().unwrap_or(0),
    })
}

/// A pax extended header's `path`, if it has one.
fn pax_path(data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    text.lines()
        .filter_map(|record| record.split_once(' ').map(|(_, kv)| kv))
        .find_map(|kv| kv.strip_prefix("path=").map(str::to_string))
}

fn skip(reader: &mut impl Read, n: u64) -> Result<(), InstallError> {
    let copied = io::copy(&mut reader.take(n), &mut io::sink())
        .map_err(|e| InstallError::io("couldn't read the package", e))?;
    if copied != n {
        return Err(InstallError::Io("the package ends early".into()));
    }
    Ok(())
}

fn padding(size: u64) -> u64 {
    (512 - size % 512) % 512
}

/// Unpacks a `.tar.gz` into `dest`, checking every entry before it is written (R5): only plain files and folders,
/// all inside `<id>/`, at most 2,000 entries and 100 MB unpacked. Links are never created.
pub(crate) fn unpack(package: &[u8], id: &str, dest: &Path) -> Result<(), InstallError> {
    let mut reader = GzDecoder::new(package);
    let read_err = |e| InstallError::io("couldn't read the package", e);
    let mut long_name: Option<String> = None;
    let mut count = 0usize;
    let mut total = 0u64;
    fs::create_dir_all(dest).map_err(|e| InstallError::io("couldn't create staging", e))?;
    loop {
        let mut block = [0u8; 512];
        reader.read_exact(&mut block).map_err(read_err)?;
        if block.iter().all(|&b| b == 0) {
            return Ok(());
        }
        let h = header(&block)?;
        // Extended headers name the next entry.
        if matches!(h.kind, b'x' | b'L' | b'g') {
            if h.size > 64 * 1024 {
                return Err(InstallError::Io(
                    "the package has an oversized header".into(),
                ));
            }
            let mut data = Vec::new();
            (&mut reader)
                .take(h.size)
                .read_to_end(&mut data)
                .map_err(read_err)?;
            skip(&mut reader, padding(h.size))?;
            match h.kind {
                b'x' => long_name = pax_path(&data).or(long_name),
                b'L' => {
                    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
                    long_name = Some(
                        String::from_utf8_lossy(data.get(..end).unwrap_or_default()).into_owned(),
                    );
                }
                _ => {}
            }
            continue;
        }
        let name = long_name.take().unwrap_or(h.name);
        let kind = match h.kind {
            b'0' | 0 | b'7' if !name.ends_with('/') => EntryKind::File,
            b'5' | b'0' | 0 => EntryKind::Dir,
            b'2' => EntryKind::Symlink,
            b'1' => EntryKind::Hardlink,
            _ => EntryKind::Other,
        };
        let entry = ArchiveEntry {
            path: name.clone(),
            kind,
            size: if kind == EntryKind::File { h.size } else { 0 },
        };
        count += 1;
        total = total.saturating_add(entry.size);
        if count > MAX_ENTRIES || total > MAX_UNPACKED_BYTES {
            return Err(InstallError::Io(
                "the package unpacks to more than Samo allows".into(),
            ));
        }
        validate_entries(id, std::slice::from_ref(&entry)).map_err(InstallError::Package)?;

        let path = name.strip_suffix('/').unwrap_or(&name);
        let rel = path
            .strip_prefix(id)
            .unwrap_or_default()
            .trim_start_matches('/');
        let to = dest.join(rel);
        let write_err = |e| InstallError::io("couldn't unpack the package", e);
        match kind {
            EntryKind::Dir => {
                fs::create_dir_all(&to).map_err(write_err)?;
                skip(&mut reader, h.size + padding(h.size))?;
            }
            EntryKind::File => {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent).map_err(write_err)?;
                }
                let mut file = fs::File::create(&to).map_err(write_err)?;
                let copied =
                    io::copy(&mut (&mut reader).take(h.size), &mut file).map_err(read_err)?;
                if copied != h.size {
                    return Err(InstallError::Io("the package ends early".into()));
                }
                let mode = if h.mode & 0o111 != 0 { 0o755 } else { 0o644 };
                fs::set_permissions(&to, fs::Permissions::from_mode(mode)).map_err(write_err)?;
                skip(&mut reader, padding(h.size))?;
            }
            // Refused by `validate_entries` above.
            EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other => {
                return Err(InstallError::Io(
                    "the package has an entry Samo refuses".into(),
                ));
            }
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

    /// A one-entry tar header, as an attacker could write it.
    fn tar_header(name: &str, kind: u8, size: usize) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000755");
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[156] = kind;
        h[257..263].copy_from_slice(b"ustar\0");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        h
    }

    fn gz(entries: &[(&str, u8, &[u8])]) -> Result<Vec<u8>, String> {
        use std::io::Write;
        let mut tar = Vec::new();
        for (name, kind, data) in entries {
            tar.extend(tar_header(name, *kind, data.len()));
            tar.extend_from_slice(data);
            tar.resize(tar.len().div_ceil(512) * 512, 0);
        }
        tar.extend([0u8; 1024]);
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(&tar).map_err(|e| e.to_string())?;
        enc.finish().map_err(|e| e.to_string())
    }

    #[test]
    fn unpack_reads_macos_tar_and_keeps_the_helper_executable() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let src = make_dev(dir.path(), "0.1.0")?;
        // A path longer than ustar's 100 bytes, so bsdtar writes an extended header.
        let deep = src.join("share").join("x".repeat(60)).join("y".repeat(60));
        fs::create_dir_all(&deep).map_err(|e| e.to_string())?;
        fs::write(deep.join("notes.txt"), "deep").map_err(|e| e.to_string())?;
        let pkg = dir.path().join("hello.tar.gz");
        let ok = std::process::Command::new("tar")
            .env("COPYFILE_DISABLE", "1")
            .arg("-C")
            .arg(dir.path().join("dev"))
            .arg("-czf")
            .arg(&pkg)
            .arg("hello")
            .status()
            .map_err(|e| e.to_string())?;
        assert!(ok.success());
        let out = dir.path().join("out");
        unpack(&fs::read(&pkg).map_err(|e| e.to_string())?, "hello", &out)
            .map_err(|e| e.to_string())?;
        let helper = fs::metadata(out.join("bin/hello")).map_err(|e| e.to_string())?;
        assert_eq!(helper.permissions().mode() & 0o777, 0o755);
        let rel = deep.strip_prefix(&src).map_err(|e| e.to_string())?;
        assert_eq!(
            fs::read_to_string(out.join(rel).join("notes.txt")).map_err(|e| e.to_string())?,
            "deep"
        );
        Ok(())
    }

    #[test]
    fn unpack_refuses_links_and_escapes_before_writing_them() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let cases: [(&str, Vec<u8>); 4] = [
            (
                "symlink",
                gz(&[("hello/", b'5', b""), ("hello/bin", b'2', b"")])?,
            ),
            ("hard link", gz(&[("hello/a", b'1', b"")])?),
            ("escape", gz(&[("hello/../../evil", b'0', b"x")])?),
            ("other id", gz(&[("other/evil", b'0', b"x")])?),
        ];
        for (what, pkg) in cases {
            let out = dir.path().join(what);
            assert!(unpack(&pkg, "hello", &out).is_err(), "{what}");
        }
        assert!(
            !dir.path().join("evil").exists()
                && !dir.path().parent().is_some_and(|p| p.join("evil").exists())
        );
        assert!(unpack(b"not a gzip at all", "hello", &dir.path().join("junk")).is_err());
        let ok = gz(&[
            ("hello/", b'5', b""),
            ("hello/extension.toml", b'0', b"id = 1"),
        ])?;
        unpack(&ok, "hello", &dir.path().join("fine")).map_err(|e| e.to_string())?;
        assert!(dir.path().join("fine/extension.toml").is_file());
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
