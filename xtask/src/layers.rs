use std::fs;
use std::path::Path;

fn layer_rank(crate_name: &str) -> Option<u8> {
    match crate_name {
        "samo-primitives" => Some(0),
        "samo-blueprint" | "samo-ext-protocol" => Some(1),
        "samo-domain" => Some(2),
        "samo-store" => Some(3),
        "samo-engine" => Some(4),
        "samo-macos" | "samo-extensions" => Some(5),
        "samo-testkit" => Some(6),
        "samo-server" | "samo-cli" | "samo-gpui" | "samo-claude" | "samo-ext-hello" => Some(7),
        _ => None,
    }
}

pub fn check_cargo_toml_edges(crate_name: &str, cargo_toml_content: &str) -> Result<(), String> {
    let Some(from_rank) = layer_rank(crate_name) else {
        return Ok(());
    };

    let mut in_deps = false;
    for raw_line in cargo_toml_content.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_deps = line == "[dependencies]" || line.starts_with("[dependencies.");
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((dep_key, _)) = line.split_once('=') {
            let dep = dep_key.trim();
            if let Some(to_rank) = layer_rank(dep) {
                if to_rank >= from_rank && crate_name != "samo-testkit" {
                    return Err(format!(
                        "layer violation: `{crate_name}` (L{from_rank}) cannot depend on `{dep}` (L{to_rank})"
                    ));
                }
                if crate_name == "samo-engine" && dep == "samo-macos" {
                    return Err(
                        "layer violation: `samo-engine` (L4) cannot depend on `samo-macos` (L5)"
                            .to_string(),
                    );
                }
            }
            if dep == "sqlx" && crate_name != "samo-store" && crate_name != "samo-testkit" {
                return Err(format!(
                    "layer violation: `{crate_name}` cannot depend on `sqlx` (only `samo-store` and `samo-testkit` may use `sqlx`)"
                ));
            }
        }
    }
    Ok(())
}

/// Every crate in the workspace and its manifest. A crate missing here goes unchecked by `layers` and `network`.
pub const MANIFESTS: [(&str, &str); 14] = [
    ("samo-primitives", "crates/samo-primitives/Cargo.toml"),
    ("samo-blueprint", "crates/samo-blueprint/Cargo.toml"),
    ("samo-ext-protocol", "crates/samo-ext-protocol/Cargo.toml"),
    ("samo-domain", "crates/samo-domain/Cargo.toml"),
    ("samo-store", "crates/samo-store/Cargo.toml"),
    ("samo-engine", "crates/samo-engine/Cargo.toml"),
    ("samo-macos", "crates/samo-macos/Cargo.toml"),
    ("samo-extensions", "crates/samo-extensions/Cargo.toml"),
    ("samo-testkit", "crates/samo-testkit/Cargo.toml"),
    ("samo-server", "apps/samo-server/Cargo.toml"),
    ("samo-cli", "apps/samo-cli/Cargo.toml"),
    ("samo-gpui", "apps/samo-gpui/Cargo.toml"),
    ("samo-claude", "extensions/claude-sessions/Cargo.toml"),
    ("samo-ext-hello", "extensions/hello/Cargo.toml"),
];

pub fn check_layers(root: &Path) -> Result<(), String> {
    for (name, rel_path) in MANIFESTS {
        let full = root.join(rel_path);
        let content = fs::read_to_string(&full)
            .map_err(|e| format!("failed to read {}: {e}", crate::display_rel(root, &full)))?;
        check_cargo_toml_edges(name, &content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_upward_dependency_from_domain_to_store() {
        let bad = r#"
[package]
name = "samo-domain"

[dependencies]
samo-store = { workspace = true }
"#;
        let res = check_cargo_toml_edges("samo-domain", bad);
        assert!(res.is_err());
    }

    #[test]
    fn rejects_sqlx_outside_store() {
        let bad = r#"
[package]
name = "samo-engine"

[dependencies]
sqlx = { workspace = true }
"#;
        let res = check_cargo_toml_edges("samo-engine", bad);
        assert!(res.is_err());
    }

    #[test]
    fn extensions_cannot_depend_on_macos() {
        let bad = r#"
[package]
name = "samo-extensions"

[dependencies]
samo-macos = { workspace = true }
"#;
        assert!(check_cargo_toml_edges("samo-extensions", bad).is_err());

        let protocol_upward = r#"
[dependencies]
samo-domain = { workspace = true }
"#;
        assert!(check_cargo_toml_edges("samo-ext-protocol", protocol_upward).is_err());

        let ok = r#"
[dependencies]
samo-primitives = { workspace = true }
samo-domain = { workspace = true }
samo-ext-protocol = { workspace = true }
"#;
        assert!(check_cargo_toml_edges("samo-extensions", ok).is_ok());
    }
}
