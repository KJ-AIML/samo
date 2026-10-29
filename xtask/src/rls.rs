use std::fs;
use std::path::Path;

pub fn check_migration_sql(
    file_name: &str,
    sql: &str,
    allowlisted_tables: &[&str],
) -> Result<(), String> {
    let lower = sql.to_ascii_lowercase();
    let mut rest = lower.as_str();

    while let Some(idx) = rest.find("create table") {
        let after = &rest[idx + "create table".len()..];
        let after = after.trim_start();
        let after = after
            .strip_prefix("if not exists")
            .map(str::trim_start)
            .unwrap_or(after);

        let Some(open_paren) = after.find('(') else {
            return Err(format!("malformed CREATE TABLE in {file_name}"));
        };
        let table_name = after[..open_paren].trim();
        let body_start = &after[open_paren + 1..];
        let Some(close_semi) = body_start.find(");") else {
            return Err(format!(
                "unclosed CREATE TABLE `{table_name}` in {file_name}"
            ));
        };
        let table_body = &body_start[..close_semi];

        if !allowlisted_tables.contains(&table_name)
            && !table_body.contains("tenant_id text not null")
        {
            return Err(format!(
                "rls violation in `{file_name}`: table `{table_name}` is missing `tenant_id TEXT NOT NULL`"
            ));
        }

        rest = &body_start[close_semi + 2..];
    }

    let mut idx_rest = lower.as_str();
    while let Some(idx) = idx_rest.find("create index") {
        let after = &idx_rest[idx + "create index".len()..];
        let Some(on_pos) = after.find(" on ") else {
            break;
        };
        let after_on = &after[on_pos + 4..];
        let Some(open_paren) = after_on.find('(') else {
            break;
        };
        let table_name = after_on[..open_paren].trim();
        let cols_part = &after_on[open_paren + 1..];
        let Some(close_paren) = cols_part.find(')') else {
            break;
        };
        let cols = cols_part[..close_paren].trim();
        if !allowlisted_tables.contains(&table_name) && !cols.starts_with("tenant_id") {
            return Err(format!(
                "rls violation in `{file_name}`: index on `{table_name}` must lead with `tenant_id`, got `({cols})`"
            ));
        }
        idx_rest = &cols_part[close_paren + 1..];
    }

    Ok(())
}

pub fn check_tx_privacy(tx_source: &str) -> Result<(), String> {
    if tx_source.contains("pub tx:") || tx_source.contains("pub(crate) tx:") {
        return Err(
            "rls violation in `crates/samo-store/src/tx.rs`: `TenantTx` inner `tx` must remain private"
                .to_string(),
        );
    }
    Ok(())
}

pub fn check_rls(root: &Path) -> Result<(), String> {
    let allowlist_path = root.join("xtask/rls-allowlist.toml");
    let allowlist_raw = fs::read_to_string(&allowlist_path).map_err(|e| {
        format!(
            "failed to read {}: {e}",
            crate::display_rel(root, &allowlist_path)
        )
    })?;
    let mut allowlisted = Vec::new();
    for part in allowlist_raw.split('"').skip(1).step_by(2) {
        allowlisted.push(part);
    }

    let mig_dir = root.join("crates/samo-store/migrations");
    if mig_dir.exists() {
        let entries =
            fs::read_dir(&mig_dir).map_err(|e| format!("failed to read migrations dir: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("sql") {
                let rel = crate::display_rel(root, &path);
                let sql =
                    fs::read_to_string(&path).map_err(|e| format!("failed to read {rel}: {e}"))?;
                check_migration_sql(&rel, &sql, &allowlisted)?;
            }
        }
    }

    let tx_file = root.join("crates/samo-store/src/tx.rs");
    if tx_file.exists() {
        let src = fs::read_to_string(&tx_file).map_err(|e| format!("failed to read tx.rs: {e}"))?;
        check_tx_privacy(&src)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_table_missing_tenant_id_or_index_not_leading_with_tenant_id() {
        let bad_table = "CREATE TABLE disk_scans (scan_id TEXT PRIMARY KEY);";
        assert!(check_migration_sql("0002.sql", bad_table, &["tenants"]).is_err());

        let bad_idx = "CREATE TABLE disk_scans (tenant_id TEXT NOT NULL, scan_id TEXT NOT NULL);\nCREATE INDEX idx_scans ON disk_scans (scan_id);";
        assert!(check_migration_sql("0002.sql", bad_idx, &["tenants"]).is_err());

        let good = "CREATE TABLE disk_scans (tenant_id TEXT NOT NULL, scan_id TEXT NOT NULL);\nCREATE INDEX idx_scans ON disk_scans (tenant_id, scan_id);";
        assert!(check_migration_sql("0002.sql", good, &["tenants"]).is_ok());
    }
}
