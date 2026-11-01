use std::fs;
use std::path::Path;

const PURE_FORBIDDEN: &[&str] = &[
    "std::fs",
    "std::net",
    "std::process",
    "tokio::",
    "sqlx::",
    "reqwest::",
];

pub fn check_pure_source(rel_path: &str, content: &str) -> Result<(), String> {
    for needle in PURE_FORBIDDEN {
        if content.contains(needle) {
            return Err(format!(
                "purity violation in `{rel_path}`: forbidden token `{needle}` in pure crate"
            ));
        }
    }
    Ok(())
}

pub fn check_cleanup_source(rel_path: &str, content: &str) -> Result<(), String> {
    for forbidden in ["remove_dir_all", "remove_file"] {
        if content.contains(forbidden) {
            return Err(format!(
                "R2 purity violation in `{rel_path}`: `{forbidden}` is forbidden in cleanup module (must move to Trash)"
            ));
        }
    }
    Ok(())
}

pub fn visit_rs_files(
    root: &Path,
    dir: &Path,
    cb: &mut dyn FnMut(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("failed to read dir {}: {e}", crate::display_rel(root, dir)))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            visit_rs_files(root, &path, cb)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let rel = crate::display_rel(root, &path);
            let content =
                fs::read_to_string(&path).map_err(|e| format!("failed to read {rel}: {e}"))?;
            cb(&rel, &content)?;
        }
    }
    Ok(())
}

pub fn check_purity(root: &Path) -> Result<(), String> {
    for pure_dir in [
        "crates/samo-primitives/src",
        "crates/samo-blueprint/src",
        "crates/samo-domain/src",
        "crates/samo-ext-protocol/src",
    ] {
        visit_rs_files(root, &root.join(pure_dir), &mut check_pure_source)?;
    }

    let cleanup_file = root.join("crates/samo-macos/src/cleanup.rs");
    if cleanup_file.exists() {
        let rel = crate::display_rel(root, &cleanup_file);
        let content =
            fs::read_to_string(&cleanup_file).map_err(|e| format!("failed to read {rel}: {e}"))?;
        check_cleanup_source(&rel, &content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_std_fs_in_domain_and_remove_dir_all_in_cleanup() {
        assert!(check_pure_source("crates/samo-domain/src/lib.rs", "use std::fs::File;").is_err());
        assert!(
            check_cleanup_source(
                "crates/samo-macos/src/cleanup.rs",
                "std::fs::remove_dir_all(p);"
            )
            .is_err()
        );
    }
}
