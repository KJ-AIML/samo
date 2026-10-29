use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn extract_quoted_command_names(source: &str, marker: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    let mut rest = source;
    while let Some(pos) = rest.find(marker) {
        let after = &rest[pos + marker.len()..];
        let Some(first_quote) = after.find('"') else {
            break;
        };
        let after_quote = &after[first_quote + 1..];
        let Some(second_quote) = after_quote.find('"') else {
            break;
        };
        let candidate = &after_quote[..second_quote];
        if candidate.contains('.') {
            set.insert(candidate.to_string());
        }
        rest = &after_quote[second_quote + 1..];
    }
    set
}

pub fn verify_spec_and_registry(spec_src: &str, registry_src: &str) -> Result<(), String> {
    let blueprint_cmds = extract_quoted_command_names(spec_src, "name:");
    let registry_cmds = extract_quoted_command_names(registry_src, "register_spec(");

    if blueprint_cmds.is_empty() {
        return Err(
            "commands-complete: no commands found in samo-blueprint/src/spec.rs".to_string(),
        );
    }

    for cmd in &blueprint_cmds {
        if !registry_cmds.contains(cmd) {
            return Err(format!(
                "commands-complete violation: blueprint command `{cmd}` is not registered in `samo-engine/src/registry.rs`"
            ));
        }
    }
    for cmd in &registry_cmds {
        if !blueprint_cmds.contains(cmd) {
            return Err(format!(
                "commands-complete violation: engine registered undeclared command `{cmd}`"
            ));
        }
    }
    Ok(())
}

pub fn check_commands_complete(root: &Path) -> Result<(), String> {
    let spec_file = root.join("crates/samo-blueprint/src/spec.rs");
    let reg_file = root.join("crates/samo-engine/src/registry.rs");
    if !spec_file.exists() || !reg_file.exists() {
        return Err("missing spec.rs or registry.rs for commands-complete gate".to_string());
    }
    let spec_src = fs::read_to_string(&spec_file).map_err(|e| e.to_string())?;
    let reg_src = fs::read_to_string(&reg_file).map_err(|e| e.to_string())?;
    verify_spec_and_registry(&spec_src, &reg_src)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unregistered_blueprint_command() {
        let spec = r#"
CommandSpec { name: "disk.scan.begin", permission: Permission::DiskScan }
CommandSpec { name: "disk.scan.finish", permission: Permission::DiskScan }
"#;
        let reg = r#"
registry.register_spec("disk.scan.begin", Box::new(BeginHandler));
"#;
        assert!(verify_spec_and_registry(spec, reg).is_err());
    }
}
