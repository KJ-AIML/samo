//! `cargo run -p xtask -- samo-package`: Samo for Windows as `target/samo-dist/Samo-<v>-windows-x86_64.zip`
//! (ADR-006): `Samo/Samo.exe` and `Samo/release.toml`, its version. It isn't Authenticode-signed (SmartScreen warns
//! once); the catalog's signature pins its SHA-256 (R11, R14).

use samo_domain::store::{
    WINDOWS, WINDOWS_EXE, WINDOWS_RELEASE, sha256_hex, windows_release_toml,
    windows_release_version,
};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::write::SimpleFileOptions;

/// Builds Samo in release and writes its zip. On the Mac, Samo is bundled by `scripts/bundle-macos.sh`.
pub fn run(root: &Path) -> Result<PathBuf, String> {
    if !cfg!(windows) {
        return Err(
            "samo-package makes Samo for Windows; on the Mac use scripts/bundle-macos.sh".into(),
        );
    }
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "samo-gpui"])
        .current_dir(root)
        .status()
        .map_err(|e| format!("couldn't run cargo build: {e}"))?;
    if !status.success() {
        return Err("building Samo failed".into());
    }
    let exe = fs::read(root.join("target").join("release").join("samo-gpui.exe"))
        .map_err(|e| format!("samo-gpui.exe: {e}"))?;
    let version = env!("CARGO_PKG_VERSION");
    let bytes = zip_samo(&exe, version)?;
    let dir = root.join("target").join("samo-dist");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let out = dir.join(format!("Samo-{version}-{WINDOWS}.zip"));
    fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "{}  {} ({} bytes)",
        sha256_hex(&bytes),
        out.display(),
        bytes.len()
    );
    Ok(out)
}

/// The zip for `exe` at `version`: `Samo/Samo.exe` and `Samo/release.toml`, deflated.
pub fn zip_samo(exe: &[u8], version: &str) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| e.to_string();
    zip.start_file(WINDOWS_EXE, options).map_err(err)?;
    zip.write_all(exe).map_err(|e| e.to_string())?;
    zip.start_file(WINDOWS_RELEASE, options).map_err(err)?;
    zip.write_all(windows_release_toml(version).as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(zip.finish().map_err(err)?.into_inner())
}

/// The version a Windows zip says it is: it must hold `Samo/Samo.exe` and a `release.toml` naming a version.
pub fn windows_zip_version(zip: &[u8]) -> Result<String, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| format!("not a zip: {e}"))?;
    if archive.by_name(WINDOWS_EXE).is_err() {
        return Err(format!("the zip has no {WINDOWS_EXE}"));
    }
    let mut text = String::new();
    archive
        .by_name(WINDOWS_RELEASE)
        .map_err(|_| format!("the zip has no {WINDOWS_RELEASE}"))?
        .take(4096)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    windows_release_version(&text).ok_or_else(|| format!("{WINDOWS_RELEASE} names no version"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_holds_samo_and_its_version() -> Result<(), String> {
        let zip = zip_samo(b"MZ not really a program", "0.5.0")?;
        assert_eq!(windows_zip_version(&zip)?, "0.5.0");
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(&zip)).map_err(|e| e.to_string())?;
        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| {
                archive
                    .by_index(i)
                    .ok()
                    .and_then(|f| f.name().ok().map(|n| n.to_string()))
            })
            .collect();
        assert_eq!(names, [WINDOWS_EXE, WINDOWS_RELEASE]);
        // A zip without the program, or without a version, isn't Samo.
        assert!(windows_zip_version(b"not a zip").is_err());
        Ok(())
    }
}
