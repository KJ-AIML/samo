//! Samo's own update (ADR-003 M5, R14). Everything is downloaded and checked here, before Samo quits: the zip's
//! hash against the signed catalog, then the unpacked app's bundle id, version and code signature. The swap
//! itself is done by the new app's binary (`samo-gpui --finish-update`), once this Samo has quit.

use crate::net::UserAction;
use crate::store::{Store, StoreFail};
use samo_domain::store::{SAMO_CAP, SamoRelease, this_platform, windows_release_version};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const BUNDLE_ID: &str = "dev.talvyn.samo";

/// The running Samo's app bundle, which an update replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppBundle {
    pub path: PathBuf,
}

/// Why this Samo can't update itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CantUpdate {
    /// Run from `cargo run` or a bare binary, not `Samo.app`.
    NotABundle,
    /// The folder holding `Samo.app` can't be written to (or macOS runs it from a read-only copy).
    ReadOnly(PathBuf),
    /// On Windows, `Samo.exe` shares its folder with other files: an update replaces the whole folder, so it never
    /// does that to one that isn't only Samo's (ADR-006, R14).
    SharedFolder(PathBuf),
}

/// What Samo for Windows' own folder holds: its program and its version.
const SAMO_FILES: [&str; 2] = ["Samo.exe", "release.toml"];

impl fmt::Display for CantUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self, cfg!(windows)) {
            (Self::NotABundle, false) => write!(
                f,
                "This Samo isn't running from Samo.app, so it can't replace itself"
            ),
            (Self::NotABundle, true) => write!(
                f,
                "This Samo isn't an installed Samo.exe, so it can't replace itself"
            ),
            (Self::ReadOnly(dir), false) => write!(
                f,
                "Samo can't write to {}, where it is. Move Samo.app to Applications first",
                dir.display()
            ),
            (Self::ReadOnly(dir), true) => write!(
                f,
                "Samo can't write to {}, where it is. Install it for this user first",
                dir.display()
            ),
            (Self::SharedFolder(dir), _) => write!(
                f,
                "Samo.exe is in {} with other files, and an update replaces its whole folder. Install it for \
                 this user first",
                dir.display()
            ),
        }
    }
}

/// Whether the Samo at `current_exe` can be replaced: it must be `<…>/X.app/Contents/MacOS/<exe>`, or on Windows
/// `<its folder>\Samo.exe`, in a folder Samo can write to.
pub fn can_update(current_exe: &Path) -> Result<AppBundle, CantUpdate> {
    can_update_on(current_exe, cfg!(windows))
}

/// `can_update` for the given OS (ADR-006 R25). On Windows the app is Samo's own folder, which an update replaces
/// whole; `cargo run`'s `samo-gpui.exe` isn't one.
pub fn can_update_on(current_exe: &Path, windows: bool) -> Result<AppBundle, CantUpdate> {
    let app = if windows {
        let named = current_exe
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Samo.exe"));
        if !named {
            return Err(CantUpdate::NotABundle);
        }
        let folder = current_exe.parent().ok_or(CantUpdate::NotABundle)?;
        // The update moves this whole folder to the Recycle Bin: only if it is Samo's alone.
        let only_samo = fs::read_dir(folder).is_ok_and(|entries| {
            entries.filter_map(Result::ok).all(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                SAMO_FILES.iter().any(|s| s.eq_ignore_ascii_case(&name))
                    && e.file_type().is_ok_and(|t| t.is_file())
            })
        });
        if !only_samo {
            return Err(CantUpdate::SharedFolder(folder.to_path_buf()));
        }
        folder
    } else {
        let macos = current_exe.parent().ok_or(CantUpdate::NotABundle)?;
        let contents = macos.parent().ok_or(CantUpdate::NotABundle)?;
        let app = contents.parent().ok_or(CantUpdate::NotABundle)?;
        let is_bundle = macos.file_name().is_some_and(|n| n == "MacOS")
            && contents.file_name().is_some_and(|n| n == "Contents")
            && app.extension().is_some_and(|e| e == "app")
            && contents.join("Info.plist").is_file();
        if !is_bundle {
            return Err(CantUpdate::NotABundle);
        }
        app
    };
    let folder = app.parent().ok_or(CantUpdate::NotABundle)?;
    let probe = folder.join(format!(".samo-update-check-{}", std::process::id()));
    match fs::create_dir(&probe) {
        Ok(()) => {
            let _ = fs::remove_dir(&probe);
            Ok(AppBundle {
                path: app.to_path_buf(),
            })
        }
        Err(_) => Err(CantUpdate::ReadOnly(folder.to_path_buf())),
    }
}

/// A new Samo, unpacked and checked, ready for `--finish-update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedUpdate {
    pub version: String,
    pub app_path: PathBuf,
}

/// Downloads Samo `release` (hash-checked against the signed catalog) and checks it, without changing anything
/// outside `staging_root`.
pub fn prepare_update(
    store: &Store,
    release: &SamoRelease,
    action: &UserAction,
    staging_root: &Path,
) -> Result<PreparedUpdate, String> {
    let package = release
        .package_on(this_platform())
        .ok_or_else(|| format!("Samo {} has no build for this PC yet", release.version))?;
    let bytes = store
        .file("Samo's download", package, SAMO_CAP, action)
        .map_err(|e: StoreFail| e.to_string())?;
    check_zip(&bytes, &release.version, staging_root)
}

/// Unpacks a downloaded Samo into `<staging_root>/<version>/` and checks it: on the Mac its `Samo.app`, on
/// Windows its folder (`check_windows_zip`).
pub fn check_zip(zip: &[u8], version: &str, staging_root: &Path) -> Result<PreparedUpdate, String> {
    if cfg!(windows) {
        check_windows_zip(zip, version, staging_root, 4 * SAMO_CAP)
    } else {
        check_mac_zip(zip, version, staging_root)
    }
}

/// The most files a Windows update may hold.
const MAX_WINDOWS_FILES: usize = 32;

/// Unpacks Samo for Windows' zip into `<staging_root>/<version>/unpacked/` and checks it, in-process (ADR-006).
/// Every entry must be a plain file or folder under `Samo/`, at a path that stays inside (no `..`, no drive, no
/// root), at most `MAX_WINDOWS_FILES` of them and `cap` bytes unpacked in all; links are refused. Then
/// `Samo/Samo.exe` must be there and `Samo/release.toml` must name `version`, the catalog's (R14). Nothing outside
/// `staging_root` is touched.
pub fn check_windows_zip(
    zip: &[u8],
    version: &str,
    staging_root: &Path,
    cap: u64,
) -> Result<PreparedUpdate, String> {
    let dir = staging_root.join(version);
    let _ = fs::remove_dir_all(&dir);
    let unpacked = dir.join("unpacked");
    fs::create_dir_all(&unpacked)
        .map_err(|e| format!("couldn't make a folder for the update: {e}"))?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|e| format!("the update isn't a zip ({e})"))?;
    if archive.len() > MAX_WINDOWS_FILES {
        return Err(format!(
            "the update holds more than {MAX_WINDOWS_FILES} files"
        ));
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("the update can't be read ({e})"))?;
        if entry.is_symlink() {
            return Err("the update holds a link".into());
        }
        let name = entry
            .enclosed_name()
            .ok_or("the update holds a path outside its folder")?;
        let in_samo = name
            .components()
            .next()
            .is_some_and(|c| c == std::path::Component::Normal("Samo".as_ref()));
        if !in_samo {
            return Err("the update holds a file outside Samo/".into());
        }
        let to = unpacked.join(&name);
        if entry.is_dir() {
            fs::create_dir_all(&to).map_err(|e| format!("couldn't unpack the update: {e}"))?;
            continue;
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("couldn't unpack the update: {e}"))?;
        }
        let mut out =
            fs::File::create(&to).map_err(|e| format!("couldn't unpack the update: {e}"))?;
        let left = cap.saturating_sub(total);
        let written = std::io::copy(&mut (&mut entry).take(left + 1), &mut out)
            .map_err(|e| format!("couldn't unpack the update: {e}"))?;
        total = total.saturating_add(written);
        if total > cap {
            return Err(format!("the update unpacks to more than {cap} bytes"));
        }
    }
    let app = unpacked.join("Samo");
    if !app.join("Samo.exe").is_file() {
        return Err("the update has no Samo.exe in it".into());
    }
    let text = fs::read_to_string(app.join("release.toml")).unwrap_or_default();
    let got = windows_release_version(&text).unwrap_or_default();
    if got != version {
        return Err(format!("the update says it is Samo {got}, not {version}"));
    }
    Ok(PreparedUpdate {
        version: version.into(),
        app_path: app,
    })
}

fn plist(app: &Path, key: &str) -> Option<String> {
    let out = Command::new("plutil")
        .args(["-extract", key, "raw"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Unpacks Samo's zip into `<staging_root>/<version>/` and checks the app in it: `Samo.app` at the top, bundle
/// id `dev.talvyn.samo`, `version` in its `Info.plist`, and a valid code signature.
fn check_mac_zip(zip: &[u8], version: &str, staging_root: &Path) -> Result<PreparedUpdate, String> {
    let dir = staging_root.join(version);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("couldn't make a folder for the update: {e}"))?;
    let zip_path = dir.join("Samo.zip");
    fs::write(&zip_path, zip).map_err(|e| format!("couldn't save the update: {e}"))?;
    let unpacked = dir.join("unpacked");
    let status = Command::new("ditto")
        .args(["-x", "-k"])
        .arg(&zip_path)
        .arg(&unpacked)
        .status()
        .map_err(|e| format!("couldn't unpack the update: {e}"))?;
    let _ = fs::remove_file(&zip_path);
    if !status.success() {
        return Err("the update couldn't be unpacked".into());
    }
    let app = unpacked.join("Samo.app");
    if !app.join("Contents/Info.plist").is_file() {
        return Err("the update has no Samo.app in it".into());
    }
    let id = plist(&app, "CFBundleIdentifier").unwrap_or_default();
    if id != BUNDLE_ID {
        return Err(format!("the update is a different app ({id})"));
    }
    let got = plist(&app, "CFBundleShortVersionString").unwrap_or_default();
    if got != version {
        return Err(format!("the update says it is Samo {got}, not {version}"));
    }
    let signed = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .output()
        .map_err(|e| format!("couldn't check the update's signature: {e}"))?;
    if !signed.status.success() {
        return Err("the update's code signature isn't valid".into());
    }
    Ok(PreparedUpdate {
        version: version.into(),
        app_path: app,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A tiny `Samo.app` (its binary is `/usr/bin/true`), ad-hoc signed like Samo's own, zipped like a release.
    fn release_zip(dir: &Path, id: &str, version: &str, sign: bool) -> Result<Vec<u8>, String> {
        let build = dir.join(format!("build-{id}-{version}-{sign}"));
        let app = build.join("Samo.app");
        fs::create_dir_all(app.join("Contents/MacOS")).map_err(|e| e.to_string())?;
        fs::copy("/usr/bin/true", app.join("Contents/MacOS/samo-gpui"))
            .map_err(|e| e.to_string())?;
        fs::write(
            app.join("Contents/Info.plist"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
                 <key>CFBundleIdentifier</key><string>{id}</string>\
                 <key>CFBundleShortVersionString</key><string>{version}</string>\
                 <key>CFBundleExecutable</key><string>samo-gpui</string>\
                 <key>CFBundlePackageType</key><string>APPL</string></dict></plist>\n"
            ),
        )
        .map_err(|e| e.to_string())?;
        if sign {
            let ok = Command::new("codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&app)
                .output()
                .map_err(|e| e.to_string())?;
            if !ok.status.success() {
                return Err(String::from_utf8_lossy(&ok.stderr).into_owned());
            }
        }
        let zip = build.join("Samo.zip");
        let ok = Command::new("ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(&app)
            .arg(&zip)
            .status()
            .map_err(|e| e.to_string())?;
        if !ok.success() {
            return Err("ditto failed".into());
        }
        fs::read(zip).map_err(|e| e.to_string())
    }

    #[test]
    fn update_refuses_wrong_bundle_or_version() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let staging = dir.path().join("Update");

        let good = release_zip(dir.path(), BUNDLE_ID, "0.3.0", true)?;
        let ready = check_zip(&good, "0.3.0", &staging)?;
        assert_eq!(ready.app_path, staging.join("0.3.0/unpacked/Samo.app"));
        assert!(!staging.join("0.3.0/Samo.zip").exists());

        let other = release_zip(dir.path(), "com.example.other", "0.3.0", true)?;
        let err = check_zip(&other, "0.3.0", &staging)
            .err()
            .unwrap_or_default();
        assert!(err.contains("different app"), "{err}");

        let err = check_zip(&good, "0.4.0", &staging)
            .err()
            .unwrap_or_default();
        assert!(err.contains("not 0.4.0"), "{err}");

        let unsigned = release_zip(dir.path(), BUNDLE_ID, "0.3.0", false)?;
        let err = check_zip(&unsigned, "0.3.0", &staging)
            .err()
            .unwrap_or_default();
        assert!(err.contains("signature"), "{err}");

        let err = check_zip(b"not a zip", "0.3.0", &staging)
            .err()
            .unwrap_or_default();
        assert!(err.contains("unpacked"), "{err}");
        Ok(())
    }

    #[test]
    fn cannot_update_from_cargo_run() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        assert_eq!(
            can_update(&dir.path().join("target/debug/samo-gpui")),
            Err(CantUpdate::NotABundle)
        );
        let fake = dir.path().join("Apps/Samo.app/Contents/MacOS");
        fs::create_dir_all(&fake).map_err(|e| e.to_string())?;
        // No Info.plist: not a bundle.
        assert_eq!(
            can_update(&fake.join("samo-gpui")),
            Err(CantUpdate::NotABundle)
        );
        fs::write(
            dir.path().join("Apps/Samo.app/Contents/Info.plist"),
            "<plist/>",
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(
            can_update(&fake.join("samo-gpui")),
            Ok(AppBundle {
                path: dir.path().join("Apps/Samo.app")
            })
        );

        use std::os::unix::fs::PermissionsExt;
        let apps = dir.path().join("Apps");
        fs::set_permissions(&apps, fs::Permissions::from_mode(0o555)).map_err(|e| e.to_string())?;
        let read_only = can_update(&fake.join("samo-gpui"));
        fs::set_permissions(&apps, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        assert_eq!(read_only, Err(CantUpdate::ReadOnly(apps)));
        Ok(())
    }
}

/// Samo for Windows' update checks, on every OS (ADR-006 R25).
#[cfg(test)]
mod windows_tests {
    use super::*;
    use samo_domain::store::windows_release_toml;
    use std::io::{Cursor, Write};

    fn zip_of(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())?;
        }
        Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
    }

    fn good(version: &str) -> Result<Vec<u8>, String> {
        zip_of(&[
            ("Samo/Samo.exe", b"MZ"),
            (
                "Samo/release.toml",
                windows_release_toml(version).as_bytes(),
            ),
        ])
    }

    const CAP: u64 = 1024 * 1024;

    #[test]
    fn a_windows_update_zip_is_checked() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = dir.path().join("Update");
        let ok = check_windows_zip(&good("0.5.0")?, "0.5.0", &root, CAP)?;
        assert_eq!(ok.version, "0.5.0");
        assert!(ok.app_path.join("Samo.exe").is_file());
        assert!(ok.app_path.ends_with(Path::new("unpacked").join("Samo")));

        let release = windows_release_toml("0.5.0");
        let mut many: Vec<(String, &[u8])> = (0..40)
            .map(|i| (format!("Samo/f{i}.txt"), b"x" as &[u8]))
            .collect();
        many.push(("Samo/Samo.exe".into(), b"MZ"));
        let many: Vec<(&str, &[u8])> = many.iter().map(|(n, b)| (n.as_str(), *b)).collect();
        let big = vec![b'x'; 2 * 1024 * 1024];
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("an older version", good("0.4.9")?),
            (
                "no Samo.exe",
                zip_of(&[("Samo/release.toml", release.as_bytes())])?,
            ),
            ("no version", zip_of(&[("Samo/Samo.exe", b"MZ")])?),
            (
                "an escape",
                zip_of(&[
                    ("Samo/Samo.exe", b"MZ"),
                    ("Samo/release.toml", release.as_bytes()),
                    ("Samo/../../evil.txt", b"x"),
                ])?,
            ),
            (
                "an absolute path",
                zip_of(&[
                    ("Samo/Samo.exe", b"MZ"),
                    ("Samo/release.toml", release.as_bytes()),
                    ("/evil.txt", b"x"),
                ])?,
            ),
            (
                "a file outside Samo/",
                zip_of(&[
                    ("Samo/Samo.exe", b"MZ"),
                    ("Samo/release.toml", release.as_bytes()),
                    ("Other/evil.txt", b"x"),
                ])?,
            ),
            ("too many files", zip_of(&many)?),
            (
                "too much to unpack",
                zip_of(&[
                    ("Samo/Samo.exe", big.as_slice()),
                    ("Samo/release.toml", release.as_bytes()),
                ])?,
            ),
            ("not a zip", b"not a zip at all".to_vec()),
        ];
        for (what, zip) in cases {
            assert!(
                check_windows_zip(&zip, "0.5.0", &root, CAP).is_err(),
                "{what}"
            );
        }
        assert!(!dir.path().join("evil.txt").exists() && !root.join("evil.txt").exists());

        // A link in the zip is never written.
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.add_symlink("Samo/Samo.exe", "C:/Windows/System32/cmd.exe", options)
            .map_err(|e| e.to_string())?;
        zip.start_file("Samo/release.toml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(release.as_bytes())
            .map_err(|e| e.to_string())?;
        let linked = zip.finish().map_err(|e| e.to_string())?.into_inner();
        assert!(
            check_windows_zip(&linked, "0.5.0", &root, CAP).is_err(),
            "a link"
        );
        Ok(())
    }

    #[test]
    fn on_windows_only_samo_exe_in_a_writable_folder_updates() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let app = dir.path().join("Programs").join("Samo");
        fs::create_dir_all(&app).map_err(|e| e.to_string())?;
        fs::write(app.join("Samo.exe"), b"MZ").map_err(|e| e.to_string())?;
        assert_eq!(
            can_update_on(&app.join("Samo.exe"), true),
            Ok(AppBundle { path: app.clone() })
        );
        // `cargo run`'s samo-gpui.exe isn't an installed Samo.
        assert_eq!(
            can_update_on(&app.join("samo-gpui.exe"), true),
            Err(CantUpdate::NotABundle)
        );
        // Its release.toml beside it is still Samo's own folder.
        fs::write(app.join("release.toml"), "version = \"0.5.0\"\n").map_err(|e| e.to_string())?;
        assert!(can_update_on(&app.join("Samo.exe"), true).is_ok());
        Ok(())
    }

    #[test]
    fn samo_exe_in_a_shared_folder_cant_update_itself() -> Result<(), String> {
        // An update replaces Samo's whole folder: Samo.exe left in Downloads must never send Downloads to the
        // Recycle Bin.
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let downloads = dir.path().join("Downloads");
        fs::create_dir_all(downloads.join("photos")).map_err(|e| e.to_string())?;
        fs::write(downloads.join("Samo.exe"), b"MZ").map_err(|e| e.to_string())?;
        fs::write(downloads.join("report.pdf"), b"%PDF").map_err(|e| e.to_string())?;
        assert_eq!(
            can_update_on(&downloads.join("Samo.exe"), true),
            Err(CantUpdate::SharedFolder(downloads.clone()))
        );
        Ok(())
    }
}
