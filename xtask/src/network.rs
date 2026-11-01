//! R6 (ADR-002): Samo goes online only when you press a button.
//!
//! gpui already brings an HTTP stack into `samo-gpui` transitively, so this gate checks what Samo's own code
//! asks for: direct dependencies and call sites.

use std::fs;
use std::path::Path;

/// HTTP client crates. Only `samo-extensions` may list one (`ureq`), for the downloads you ask for.
const HTTP_CLIENTS: &[&str] = &[
    "ureq",
    "reqwest",
    "isahc",
    "attohttpc",
    "curl",
    "surf",
    "minreq",
    "ehttp",
];

/// The only file allowed to say a click happened, so a download can't start on a timer.
const CLICK_SITE: &str = "apps/samo-gpui/src/extensions_page.rs";

fn allowed_http(crate_name: &str, dep: &str) -> bool {
    crate_name == "samo-extensions" && dep == "ureq"
}

/// Every dependency a manifest lists under `[dependencies]`, `[target.….dependencies]` or `[dependencies.<name>]`.
fn direct_dependencies(cargo_toml: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for raw_line in cargo_toml.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            let header = line.trim_start_matches('[').trim_end_matches(']').trim();
            if let Some(name) = header.strip_prefix("dependencies.") {
                deps.push(name.trim_matches('"').to_string());
                in_deps = false;
            } else {
                in_deps = header == "dependencies" || header.ends_with(".dependencies");
            }
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            deps.push(key.trim().trim_matches('"').to_string());
        }
    }
    deps
}

pub fn check_manifest(crate_name: &str, cargo_toml: &str) -> Result<(), String> {
    for dep in direct_dependencies(cargo_toml) {
        if HTTP_CLIENTS.contains(&dep.as_str()) && !allowed_http(crate_name, &dep) {
            return Err(format!(
                "R6 network violation: `{crate_name}` depends on HTTP client `{dep}` (only `samo-extensions` may use `ureq`)"
            ));
        }
    }
    Ok(())
}

/// The Mac app must not use gpui's built-in HTTP client.
pub fn check_gpui_source(rel_path: &str, content: &str) -> Result<(), String> {
    if content.contains("http_client(") {
        return Err(format!(
            "R6 network violation in `{rel_path}`: the Mac app must not use gpui's HTTP client"
        ));
    }
    Ok(())
}

/// A `UserAction` (what every download needs) may only be made where the user clicks.
pub fn check_user_action(rel_path: &str, content: &str) -> Result<(), String> {
    if rel_path != CLICK_SITE && content.contains("UserAction::clicked(") {
        return Err(format!(
            "R6 network violation in `{rel_path}`: `UserAction::clicked()` may only appear in `{CLICK_SITE}`"
        ));
    }
    Ok(())
}

/// Claude Sessions never goes online.
pub fn check_claude_source(rel_path: &str, content: &str) -> Result<(), String> {
    for needle in ["std::net", "TcpStream", "UdpSocket"] {
        if content.contains(needle) {
            return Err(format!(
                "R6 network violation in `{rel_path}`: Claude Sessions must not use `{needle}`"
            ));
        }
    }
    Ok(())
}

pub fn check_network(root: &Path) -> Result<(), String> {
    for (name, rel_path) in crate::layers::MANIFESTS {
        let full = root.join(rel_path);
        let content = fs::read_to_string(&full)
            .map_err(|e| format!("failed to read {}: {e}", crate::display_rel(root, &full)))?;
        check_manifest(name, &content)?;
    }

    crate::purity::visit_rs_files(
        root,
        &root.join("apps/samo-gpui/src"),
        &mut check_gpui_source,
    )?;
    for dir in ["apps", "crates", "extensions"] {
        crate::purity::visit_rs_files(root, &root.join(dir), &mut check_user_action)?;
    }
    crate::purity::visit_rs_files(
        root,
        &root.join("extensions/claude-sessions"),
        &mut check_claude_source,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_http_crate_outside_extensions() {
        let ureq = r#"
[dependencies]
ureq = { version = "3", default-features = false }
"#;
        assert!(check_manifest("samo-extensions", ureq).is_ok());
        assert!(check_manifest("samo-gpui", ureq).is_err());
        assert!(check_manifest("samo-claude", ureq).is_err());

        let reqwest = "[dependencies]\nreqwest = \"0.12\"\n";
        assert!(check_manifest("samo-extensions", reqwest).is_err());

        let table = "[dependencies.reqwest]\nversion = \"0.12\"\n";
        assert!(check_manifest("samo-cli", table).is_err());

        let target = "[target.'cfg(target_os = \"macos\")'.dependencies]\ncurl = \"0.4\"\n";
        assert!(check_manifest("samo-macos", target).is_err());

        // A dev-dependency is not shipped, and a server is not a client.
        let fine = "[dependencies]\naxum = \"0.8\"\n\n[dev-dependencies]\nreqwest = \"0.12\"\n";
        assert!(check_manifest("samo-server", fine).is_ok());
    }

    #[test]
    fn rejects_gpui_http_client_call() {
        assert!(
            check_gpui_source("apps/samo-gpui/src/main.rs", "let c = cx.http_client();").is_err()
        );
        assert!(check_gpui_source("apps/samo-gpui/src/main.rs", "let x = 1;").is_ok());
    }

    #[test]
    fn rejects_user_action_outside_page() {
        let call = "let a = UserAction::clicked();";
        assert!(check_user_action(CLICK_SITE, call).is_ok());
        assert!(check_user_action("apps/samo-gpui/src/hub.rs", call).is_err());
        assert!(check_user_action("crates/samo-extensions/src/host.rs", call).is_err());
        // Defining it is fine anywhere.
        assert!(
            check_user_action(
                "crates/samo-extensions/src/net.rs",
                "pub fn clicked() -> Self"
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_std_net_in_claude_sessions() {
        let path = "extensions/claude-sessions/src/serve.rs";
        assert!(check_claude_source(path, "use std::net::TcpStream;").is_err());
        assert!(check_claude_source(path, "let s = UdpSocket::bind(a);").is_err());
        assert!(check_claude_source(path, "use std::fs;").is_ok());
    }
}
