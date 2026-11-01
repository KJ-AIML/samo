//! Samo's own update (ADR-003 M5, R14). Everything is downloaded and checked here, before Samo quits: the zip's
//! hash against the signed catalog, then the unpacked app's bundle id, version and code signature. The swap
//! itself is done by the new app's binary (`samo-gpui --finish-update`), once this Samo has quit.

use crate::net::UserAction;
use crate::store::{Store, StoreFail};
use samo_domain::store::{SAMO_CAP, SamoRelease};
use std::env;
use std::fmt;
use std::fs;
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
}

impl fmt::Display for CantUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotABundle => write!(
                f,
                "This Samo isn't running from Samo.app, so it can't replace itself"
            ),
            Self::ReadOnly(dir) => write!(
                f,
                "Samo can't write to {}, where it is. Move Samo.app to Applications first",
                dir.display()
            ),
        }
    }
}

/// Whether the Samo at `current_exe` can be replaced: it must be `<…>/X.app/Contents/MacOS/<exe>` in a folder
/// Samo can write to.
pub fn can_update(current_exe: &Path) -> Result<AppBundle, CantUpdate> {
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

/// `~/Library/Caches/Samo/Update/`, where updates are unpacked.
pub fn staging_root() -> Option<PathBuf> {
    let home = env::var_os("HOME").filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join("Library/Caches/Samo/Update"))
}

/// Downloads Samo `release` (hash-checked against the signed catalog) and checks it, without changing anything
/// outside `staging_root`.
pub fn prepare_update(
    store: &Store,
    release: &SamoRelease,
    action: &UserAction,
    staging_root: &Path,
) -> Result<PreparedUpdate, String> {
    let bytes = store
        .file("Samo's download", &release.package, SAMO_CAP, action)
        .map_err(|e: StoreFail| e.to_string())?;
    check_zip(&bytes, &release.version, staging_root)
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
pub fn check_zip(zip: &[u8], version: &str, staging_root: &Path) -> Result<PreparedUpdate, String> {
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

#[cfg(test)]
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
