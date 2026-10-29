use crate::tx::{SystemTx, TenantTx};
use samo_primitives::{
    ActorId, AuditId, ErrorCode, FileNodeIdentity, PhosphorTheme, ProcessIdentity, SamoError,
    SamoResult, ScanId, ScanProgressStatus, ScanSnapshot, TenantId,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantSettingsRecord {
    pub tenant_id: String,
    pub theme: PhosphorTheme,
    pub default_scan_root: String,
    pub sample_interval_ms: u64,
    pub include_self_process: bool,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanHistoryItem {
    pub scan_id: String,
    pub root_path: String,
    pub volume_name: String,
    pub status: ScanProgressStatus,
    pub scanned_bytes: u64,
    pub scanned_files: u64,
    pub volume_used_bytes: u64,
    pub volume_total_bytes: u64,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupActionRecord {
    pub action_id: String,
    pub scan_id: String,
    pub target_path: String,
    pub identity: FileNodeIdentity,
    pub status: String,
    pub trashed_path: Option<String>,
    pub reclaimed_bytes: u64,
}

fn parse_theme(raw: &str) -> PhosphorTheme {
    match raw {
        "light" => PhosphorTheme::Light,
        "green" => PhosphorTheme::Green,
        "amber" => PhosphorTheme::Amber,
        "blue" => PhosphorTheme::Blue,
        "mono" => PhosphorTheme::Mono,
        _ => PhosphorTheme::Dark,
    }
}

fn theme_to_str(theme: PhosphorTheme) -> &'static str {
    match theme {
        PhosphorTheme::Dark => "dark",
        PhosphorTheme::Light => "light",
        PhosphorTheme::Green => "green",
        PhosphorTheme::Amber => "amber",
        PhosphorTheme::Blue => "blue",
        PhosphorTheme::Mono => "mono",
    }
}

fn parse_status(raw: &str) -> ScanProgressStatus {
    match raw {
        "scanning" => ScanProgressStatus::Scanning,
        "completed" => ScanProgressStatus::Completed,
        "interrupted" => ScanProgressStatus::Interrupted,
        _ => ScanProgressStatus::Failed,
    }
}

pub async fn ensure_tenant(
    tx: &mut SystemTx<'_>,
    tenant_id: &TenantId,
    display_name: &str,
    now_ms: i64,
) -> SamoResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO tenants (tenant_id, display_name, created_at_ms) VALUES (?1, ?2, ?3)",
    )
    .bind(tenant_id.as_str())
    .bind(display_name)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(())
}

/// Marks any leftover `scanning` or `pending` rows as `interrupted` on server startup.
pub async fn reconcile_interrupted_scans(tx: &mut SystemTx<'_>, now_ms: i64) -> SamoResult<u64> {
    let res = sqlx::query(
        "UPDATE disk_scans SET status = 'interrupted', completed_at_ms = ?1 WHERE status = 'scanning'",
    )
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(res.rows_affected())
}

pub async fn get_or_init_settings(
    tx: &mut TenantTx<'_>,
    default_root: &str,
    now_ms: i64,
) -> SamoResult<TenantSettingsRecord> {
    let tid = tx.tenant_id().as_str().to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO tenant_settings (tenant_id, theme, default_scan_root, sample_interval_ms, include_self_process, updated_at_ms) VALUES (?1, 'dark', ?2, 1000, 0, ?3)",
    )
    .bind(&tid)
    .bind(default_root)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    let row = sqlx::query(
        "SELECT theme, default_scan_root, sample_interval_ms, include_self_process, updated_at_ms FROM tenant_settings WHERE tenant_id = ?1",
    )
    .bind(&tid)
    .fetch_one(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    let theme_str: String = row.get("theme");
    let root: String = row.get("default_scan_root");
    let interval: i64 = row.get("sample_interval_ms");
    let include_self: i64 = row.get("include_self_process");
    let updated: i64 = row.get("updated_at_ms");

    Ok(TenantSettingsRecord {
        tenant_id: tid,
        theme: parse_theme(&theme_str),
        default_scan_root: root,
        sample_interval_ms: interval.max(250) as u64,
        include_self_process: include_self != 0,
        updated_at_ms: updated,
    })
}

pub async fn update_theme(
    tx: &mut TenantTx<'_>,
    theme: PhosphorTheme,
    include_self_process: Option<bool>,
    now_ms: i64,
) -> SamoResult<TenantSettingsRecord> {
    let current = get_or_init_settings(tx, "/", now_ms).await?;
    let tid = tx.tenant_id().as_str().to_string();
    let new_self = include_self_process.unwrap_or(current.include_self_process);

    sqlx::query(
        "UPDATE tenant_settings SET theme = ?1, include_self_process = ?2, updated_at_ms = ?3 WHERE tenant_id = ?4",
    )
    .bind(theme_to_str(theme))
    .bind(i64::from(new_self))
    .bind(now_ms)
    .bind(&tid)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    get_or_init_settings(tx, &current.default_scan_root, now_ms).await
}

pub async fn insert_scan_begin(
    tx: &mut TenantTx<'_>,
    scan_id: &ScanId,
    root_path: &str,
    now_ms: i64,
) -> SamoResult<()> {
    let tid = tx.tenant_id().as_str().to_string();
    sqlx::query(
        "INSERT INTO disk_scans (tenant_id, scan_id, root_path, status, started_at_ms) VALUES (?1, ?2, ?3, 'scanning', ?4)",
    )
    .bind(&tid)
    .bind(scan_id.as_str())
    .bind(root_path)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(())
}

pub async fn finish_scan_snapshot(
    tx: &mut TenantTx<'_>,
    snapshot: &ScanSnapshot,
    now_ms: i64,
) -> SamoResult<()> {
    let tid = tx.tenant_id().as_str().to_string();
    let status_str = match snapshot.status {
        ScanProgressStatus::Completed => "completed",
        ScanProgressStatus::Scanning => "scanning",
        ScanProgressStatus::Interrupted => "interrupted",
        ScanProgressStatus::Failed => "failed",
    };

    sqlx::query(
        "UPDATE disk_scans SET status = ?1, completed_at_ms = ?2 WHERE tenant_id = ?3 AND scan_id = ?4",
    )
    .bind(status_str)
    .bind(now_ms)
    .bind(&tid)
    .bind(&snapshot.scan_id)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    let snapshot_json = serde_json::to_string(snapshot)
        .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    sqlx::query(
        "INSERT OR REPLACE INTO disk_scan_snapshots (tenant_id, scan_id, root_path, volume_name, scanned_bytes, scanned_files, volume_used_bytes, volume_total_bytes, snapshot_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&tid)
    .bind(&snapshot.scan_id)
    .bind(&snapshot.root_path)
    .bind(&snapshot.volume_name)
    .bind(snapshot.scanned_bytes as i64)
    .bind(snapshot.scanned_files as i64)
    .bind(snapshot.volume_used_bytes as i64)
    .bind(snapshot.volume_total_bytes as i64)
    .bind(snapshot_json)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    Ok(())
}

pub async fn get_scan_snapshot(
    tx: &mut TenantTx<'_>,
    scan_id: Option<&str>,
) -> SamoResult<Option<ScanSnapshot>> {
    let tid = tx.tenant_id().as_str().to_string();
    let row_opt = if let Some(sid) = scan_id {
        sqlx::query(
            "SELECT snapshot_json FROM disk_scan_snapshots WHERE tenant_id = ?1 AND scan_id = ?2",
        )
        .bind(&tid)
        .bind(sid)
        .fetch_optional(tx.conn())
        .await
        .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?
    } else {
        sqlx::query(
            "SELECT snapshot_json FROM disk_scan_snapshots WHERE tenant_id = ?1 ORDER BY created_at_ms DESC LIMIT 1",
        )
        .bind(&tid)
        .fetch_optional(tx.conn())
        .await
        .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?
    };

    let Some(row) = row_opt else {
        return Ok(None);
    };
    let json_str: String = row.get("snapshot_json");
    let snap: ScanSnapshot = serde_json::from_str(&json_str)
        .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(Some(snap))
}

pub async fn list_scan_history(tx: &mut TenantTx<'_>) -> SamoResult<Vec<ScanHistoryItem>> {
    let tid = tx.tenant_id().as_str().to_string();
    let rows = sqlx::query(
        "SELECT s.scan_id, s.root_path, s.status, s.started_at_ms, s.completed_at_ms,
                COALESCE(p.volume_name, 'Macintosh HD') AS volume_name,
                COALESCE(p.scanned_bytes, 0) AS scanned_bytes,
                COALESCE(p.scanned_files, 0) AS scanned_files,
                COALESCE(p.volume_used_bytes, 0) AS volume_used_bytes,
                COALESCE(p.volume_total_bytes, 0) AS volume_total_bytes
         FROM disk_scans s
         LEFT JOIN disk_scan_snapshots p ON p.tenant_id = s.tenant_id AND p.scan_id = s.scan_id
         WHERE s.tenant_id = ?1
         ORDER BY s.started_at_ms DESC
         LIMIT 50",
    )
    .bind(&tid)
    .fetch_all(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let status_raw: String = row.get("status");
        items.push(ScanHistoryItem {
            scan_id: row.get("scan_id"),
            root_path: row.get("root_path"),
            volume_name: row.get("volume_name"),
            status: parse_status(&status_raw),
            scanned_bytes: (row.get::<i64, _>("scanned_bytes")).max(0) as u64,
            scanned_files: (row.get::<i64, _>("scanned_files")).max(0) as u64,
            volume_used_bytes: (row.get::<i64, _>("volume_used_bytes")).max(0) as u64,
            volume_total_bytes: (row.get::<i64, _>("volume_total_bytes")).max(0) as u64,
            started_at_ms: row.get("started_at_ms"),
            completed_at_ms: row.get("completed_at_ms"),
        });
    }
    Ok(items)
}

pub async fn insert_cleanup_begin(
    tx: &mut TenantTx<'_>,
    action_id: &str,
    scan_id: &str,
    target_path: &str,
    identity: FileNodeIdentity,
    now_ms: i64,
) -> SamoResult<()> {
    let tid = tx.tenant_id().as_str().to_string();
    sqlx::query(
        "INSERT INTO cleanup_actions (tenant_id, action_id, scan_id, target_path, device_id, inode, status, reclaimed_bytes, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', 0, ?7)",
    )
    .bind(&tid)
    .bind(action_id)
    .bind(scan_id)
    .bind(target_path)
    .bind(identity.device_id as i64)
    .bind(identity.inode as i64)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(())
}

pub async fn finish_cleanup_action(
    tx: &mut TenantTx<'_>,
    action_id: &str,
    status: &str,
    trashed_path: Option<&str>,
    reclaimed_bytes: u64,
    now_ms: i64,
) -> SamoResult<()> {
    let tid = tx.tenant_id().as_str().to_string();
    sqlx::query(
        "UPDATE cleanup_actions SET status = ?1, trashed_path = ?2, reclaimed_bytes = ?3, completed_at_ms = ?4 WHERE tenant_id = ?5 AND action_id = ?6",
    )
    .bind(status)
    .bind(trashed_path)
    .bind(reclaimed_bytes as i64)
    .bind(now_ms)
    .bind(&tid)
    .bind(action_id)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(())
}

pub async fn insert_process_action_begin(
    tx: &mut TenantTx<'_>,
    action_id: &str,
    identity: ProcessIdentity,
    signal: &str,
    now_ms: i64,
) -> SamoResult<()> {
    let tid = tx.tenant_id().as_str().to_string();
    sqlx::query(
        "INSERT INTO process_actions (tenant_id, action_id, pid, start_time_secs, signal, status, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
    )
    .bind(&tid)
    .bind(action_id)
    .bind(i64::from(identity.pid))
    .bind(identity.start_time_secs as i64)
    .bind(signal)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(())
}

pub async fn finish_process_action(
    tx: &mut TenantTx<'_>,
    action_id: &str,
    status: &str,
    now_ms: i64,
) -> SamoResult<()> {
    let tid = tx.tenant_id().as_str().to_string();
    sqlx::query(
        "UPDATE process_actions SET status = ?1, completed_at_ms = ?2 WHERE tenant_id = ?3 AND action_id = ?4",
    )
    .bind(status)
    .bind(now_ms)
    .bind(&tid)
    .bind(action_id)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    Ok(())
}

pub async fn append_audit_and_outbox(
    tx: &mut TenantTx<'_>,
    actor_id: &ActorId,
    surface: &str,
    command_name: &str,
    redacted_payload: &str,
    now_ms: i64,
) -> SamoResult<AuditId> {
    let tid = tx.tenant_id().as_str().to_string();
    let audit_id = AuditId::generate();
    let event_id = format!("evt_{}", uuid::Uuid::new_v4().simple());

    sqlx::query(
        "INSERT INTO audit_log (tenant_id, audit_id, actor_id, surface, command_name, redacted_payload, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&tid)
    .bind(audit_id.as_str())
    .bind(actor_id.as_str())
    .bind(surface)
    .bind(command_name)
    .bind(redacted_payload)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    sqlx::query(
        "INSERT INTO outbox_events (tenant_id, event_id, event_type, payload_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&tid)
    .bind(&event_id)
    .bind(command_name)
    .bind(redacted_payload)
    .bind(now_ms)
    .execute(tx.conn())
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

    Ok(audit_id)
}

pub async fn count_audit_entries(tx: &mut TenantTx<'_>) -> SamoResult<u64> {
    let tid = tx.tenant_id().as_str().to_string();
    let row = sqlx::query("SELECT COUNT(*) AS cnt FROM audit_log WHERE tenant_id = ?1")
        .bind(&tid)
        .fetch_one(tx.conn())
        .await
        .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
    let cnt: i64 = row.get("cnt");
    Ok(cnt.max(0) as u64)
}
