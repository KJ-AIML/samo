//! Swift extensions (ADR-004): the never-crash gate that stands in for Rust's deny lints, and building and testing
//! them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What would crash a helper: Swift's equivalents of `unwrap`, `expect` and `panic!`.
pub const CRASHERS: &[&str] = &[
    "try!",
    "as!",
    "fatalError(",
    "precondition(",
    "preconditionFailure(",
];

/// Every `.swift` file under `dir`, with its path relative to `root`; build output (`.build`) is skipped.
pub fn visit_swift_files(
    root: &Path,
    dir: &Path,
    cb: &mut dyn FnMut(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("failed to read dir {}: {e}", crate::display_rel(root, dir)))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_dir() {
            if name != ".build" {
                visit_swift_files(root, &path, cb)?;
            }
        } else if name.ends_with(".swift") {
            let rel = crate::display_rel(root, &path);
            let content =
                fs::read_to_string(&path).map_err(|e| format!("failed to read {rel}: {e}"))?;
            cb(&rel, &content)?;
        }
    }
    Ok(())
}

/// Tests may crash on purpose, and may start processes.
pub fn is_test(rel_path: &str) -> bool {
    rel_path.contains("/Tests/")
}

/// The code of a Swift file, without `//` comment lines, so prose about a rule doesn't break it.
pub fn code(content: &str) -> String {
    content
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn check_crashers(rel_path: &str, content: &str) -> Result<(), String> {
    if is_test(rel_path) {
        return Ok(());
    }
    let code = code(content);
    for crasher in CRASHERS {
        if code.contains(crasher) {
            return Err(format!(
                "never-crash violation in `{rel_path}`: `{crasher}` (use `do`/`catch`, `as?`, or return an error)"
            ));
        }
    }
    Ok(())
}

pub fn check_swift(root: &Path) -> Result<(), String> {
    visit_swift_files(root, &root.join("extensions"), &mut check_crashers)
}

/// The Swift packages among the extensions.
pub fn packages(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(root.join("extensions"))
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("Package.swift").is_file())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// `swift build` for one product of the package in `dir`.
pub fn build_args(dir: &Path, product: &str, release: bool) -> Vec<String> {
    vec![
        "build".into(),
        "--package-path".into(),
        dir.to_string_lossy().into_owned(),
        "-c".into(),
        if release { "release" } else { "debug" }.into(),
        "--product".into(),
        product.into(),
    ]
}

fn swift(args: &[String], what: &str) -> Result<(), String> {
    let status = Command::new("swift").args(args).status().map_err(|e| {
        format!("couldn't run `swift` for {what} ({e}); Swift extensions need Xcode or its command line tools")
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed"))
    }
}

/// Builds and tests every Swift extension. On a Mac a missing toolchain fails, rather than skipping anything. The
/// Swift extensions are AppKit apps that build only on macOS, so elsewhere this says so and skips them (ADR-006 R30).
pub fn run_swift_tests(root: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        println!("    swift: the Swift extensions build only on macOS; skipped on this OS");
        return Ok(());
    }
    for dir in packages(root) {
        let path = dir.to_string_lossy().into_owned();
        let rel = crate::display_rel(root, &dir);
        swift(
            &["build".into(), "--package-path".into(), path.clone()],
            &format!("swift build ({rel})"),
        )?;
        swift(
            &["test".into(), "--package-path".into(), path],
            &format!("swift test ({rel})"),
        )?;
    }
    Ok(())
}

/// Where `swift build` put the products of the package in `dir`.
pub fn bin_path(dir: &Path, release: bool) -> Result<PathBuf, String> {
    let out = Command::new("swift")
        .args([
            "build",
            "--package-path",
            &dir.to_string_lossy(),
            "-c",
            if release { "release" } else { "debug" },
            "--show-bin-path",
        ])
        .output()
        .map_err(|e| format!("couldn't run `swift` ({e})"))?;
    if !out.status.success() {
        return Err("swift couldn't say where it builds".into());
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swift_gate_rejects_crashers() {
        let path = "extensions/shots/Sources/SamoShots/Capture/Shot.swift";
        for line in [
            "let data = try! Data(contentsOf: url)",
            "let n = value as! NSNumber",
            "fatalError(\"no screen\")",
            "precondition(count > 0)",
            "preconditionFailure(\"never\")",
        ] {
            assert!(check_crashers(path, line).is_err(), "{line}");
        }
        assert!(check_crashers(path, "let data = try? Data(contentsOf: url)").is_ok());
        assert!(check_crashers(path, "guard let n = value as? NSNumber else { return }").is_ok());
        // Prose in a comment, and tests, may say anything.
        assert!(check_crashers(path, "// never use try! here").is_ok());
        assert!(
            check_crashers("extensions/shots/Tests/SamoShotsTests/A.swift", "try! x()").is_ok()
        );
    }

    #[test]
    fn swift_package_command() {
        let args = build_args(Path::new("extensions/shots"), "samo-shots", true);
        assert_eq!(
            args,
            [
                "build",
                "--package-path",
                "extensions/shots",
                "-c",
                "release",
                "--product",
                "samo-shots"
            ]
        );
        assert!(build_args(Path::new("x"), "y", false).contains(&"debug".to_string()));
    }
}
