//! `cargo run -p xtask -- ext-dev <folder>`: builds an extension's Rust helper and lays it out in
//! `target/ext-dev/<id>/`, ready for `SAMO_EXT_DEV_DIR`, on the Mac and on Windows (ADR-006).

use samo_ext_protocol::Manifest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds the helper of the extension in `folder` and lays it out in `target/ext-dev/<id>/`.
pub fn run(root: &Path, folder: &Path) -> Result<PathBuf, String> {
    let manifest_path = folder.join("extension.toml");
    let text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("couldn't read {}: {e}", manifest_path.display()))?;
    let manifest = Manifest::parse(&text).map_err(|e| e.to_string())?;
    let bin = Path::new(&manifest.helper)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("the manifest's helper has no file name")?;
    let cargo = fs::read_to_string(folder.join("Cargo.toml"))
        .map_err(|e| format!("{} has no Cargo.toml: {e}", folder.display()))?;
    let package = crate::store::field(&cargo, "name").ok_or("Cargo.toml has no package name")?;
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", &package, "--bin", &bin])
        .current_dir(root)
        .status()
        .map_err(|e| format!("couldn't run cargo build: {e}"))?;
    if !status.success() {
        return Err(format!("building {package} failed"));
    }
    let suffix = std::env::consts::EXE_SUFFIX;
    let built = root
        .join("target")
        .join("release")
        .join(format!("{bin}{suffix}"));
    let out = root.join("target").join("ext-dev").join(&manifest.id);
    lay_out(&manifest_path, &built, &out, &manifest.helper, suffix)?;
    let dev = root.join("target").join("ext-dev");
    println!("Laid out {} in {}", manifest.id, out.display());
    if cfg!(windows) {
        println!(
            "Run: $env:SAMO_EXT_DEV_DIR=\"{}\"; cargo run --release -p samo-gpui",
            dev.display()
        );
    } else {
        println!(
            "Run: SAMO_EXT_DEV_DIR=\"{}\" cargo run --release -p samo-gpui",
            dev.display()
        );
    }
    Ok(out)
}

/// Writes `out/extension.toml` and a fresh copy of `built` at `out/<helper><exe_suffix>` (`.exe` on Windows),
/// replacing only those two files. The old helper is removed first, so a hard link left by an older layout is never
/// written through, and the copy is a file of its own: Samo refuses hard links in a package (ADR-002 R5).
pub fn lay_out(
    manifest: &Path,
    built: &Path,
    out: &Path,
    helper: &str,
    exe_suffix: &str,
) -> Result<(), String> {
    let dest = out.join(format!("{helper}{exe_suffix}"));
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("couldn't create {}: {e}", parent.display()))?;
    }
    fs::copy(manifest, out.join("extension.toml"))
        .map_err(|e| format!("couldn't copy extension.toml: {e}"))?;
    let _ = fs::remove_file(&dest);
    fs::copy(built, &dest).map_err(|e| format!("couldn't copy {}: {e}", built.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn io(e: impl std::fmt::Display) -> String {
        e.to_string()
    }

    #[test]
    fn lay_out_makes_a_fresh_exe_copy() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(io)?;
        let manifest = dir.path().join("extension.toml");
        fs::write(&manifest, "id = \"hello\"\n").map_err(io)?;
        let built = dir.path().join("built");
        fs::write(&built, "program").map_err(io)?;
        for (suffix, name) in [
            ("", "bin/samo-ext-hello"),
            (".exe", "bin/samo-ext-hello.exe"),
        ] {
            let out = dir.path().join(format!("out{suffix}"));
            // A stale hard link from an older layout must not be written through.
            fs::create_dir_all(out.join("bin")).map_err(io)?;
            let stale = dir.path().join(format!("stale{suffix}"));
            fs::write(&stale, "old").map_err(io)?;
            fs::hard_link(&stale, out.join(name)).map_err(io)?;

            lay_out(&manifest, &built, &out, "bin/samo-ext-hello", suffix)?;
            assert_eq!(fs::read_to_string(out.join(name)).map_err(io)?, "program");
            assert_eq!(fs::read_to_string(&stale).map_err(io)?, "old", "{suffix}");
            assert_eq!(
                fs::read_to_string(out.join("extension.toml")).map_err(io)?,
                "id = \"hello\"\n"
            );
        }
        Ok(())
    }
}
