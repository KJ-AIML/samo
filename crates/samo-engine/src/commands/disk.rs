use crate::command::{BoxCommandFuture, CommandHandler};
use crate::ctx::CommandCtx;
use samo_domain::{is_protected_system_path, validate_trash_target};
use samo_primitives::{ErrorCode, FileNodeIdentity, SamoError, ScanId, ScanSnapshot};
use samo_store::{
    TenantTx, finish_cleanup_action, finish_scan_snapshot, get_scan_snapshot, insert_cleanup_begin,
    insert_scan_begin, list_scan_history,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct DiskScanBeginInput {
    pub root_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiskScanBeginOutput {
    pub scan_id: String,
    pub root_path: String,
}

pub struct DiskScanBeginCmd;

impl CommandHandler for DiskScanBeginCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: DiskScanBeginInput = serde_json::from_value(payload)
                .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?;
            let trimmed = input.root_path.trim();
            if trimmed.is_empty() || !trimmed.starts_with('/') {
                return Err(SamoError::new(
                    ErrorCode::InvalidInput,
                    "scan root_path must be an absolute path starting with `/`",
                ));
            }
            let scan_id = ScanId::generate();
            insert_scan_begin(tx, &scan_id, trimmed, ctx.now_ms).await?;
            serde_json::to_value(DiskScanBeginOutput {
                scan_id: scan_id.as_str().to_string(),
                root_path: trimmed.to_string(),
            })
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct DiskScanFinishInput {
    pub snapshot: ScanSnapshot,
}

pub struct DiskScanFinishCmd;

impl CommandHandler for DiskScanFinishCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: DiskScanFinishInput = serde_json::from_value(payload)
                .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?;
            finish_scan_snapshot(tx, &input.snapshot, ctx.now_ms).await?;
            serde_json::to_value(&input.snapshot)
                .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}

pub struct DiskScanListCmd;

impl CommandHandler for DiskScanListCmd {
    fn handle<'a>(
        &'a self,
        _ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        _payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let items = list_scan_history(tx).await?;
            serde_json::to_value(items)
                .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct DiskScanGetInput {
    pub scan_id: Option<String>,
}

pub struct DiskScanGetCmd;

impl CommandHandler for DiskScanGetCmd {
    fn handle<'a>(
        &'a self,
        _ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: DiskScanGetInput = if payload.is_null() {
                DiskScanGetInput::default()
            } else {
                serde_json::from_value(payload)
                    .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?
            };
            let snap = get_scan_snapshot(tx, input.scan_id.as_deref()).await?;
            serde_json::to_value(snap)
                .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct DiskCleanupBeginInput {
    pub scan_id: String,
    pub target_path: String,
    pub scanned_identity: FileNodeIdentity,
    pub live_identity: Option<FileNodeIdentity>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiskCleanupBeginOutput {
    pub action_id: String,
    pub target_path: String,
}

pub struct DiskCleanupBeginCmd;

impl CommandHandler for DiskCleanupBeginCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: DiskCleanupBeginInput = serde_json::from_value(payload)
                .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?;

            if is_protected_system_path(&input.target_path) {
                return Err(SamoError::new(
                    ErrorCode::ProtectedPathDenied,
                    format!("R2 guard blocked protected path `{}`", input.target_path),
                ));
            }
            validate_trash_target(
                &input.target_path,
                input.scanned_identity,
                input.live_identity,
            )?;

            let action_id = format!("cln_{}", uuid::Uuid::new_v4().simple());
            insert_cleanup_begin(
                tx,
                &action_id,
                &input.scan_id,
                &input.target_path,
                input.scanned_identity,
                ctx.now_ms,
            )
            .await?;

            serde_json::to_value(DiskCleanupBeginOutput {
                action_id,
                target_path: input.target_path,
            })
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct DiskCleanupFinishInput {
    pub action_id: String,
    pub status: String,
    pub trashed_path: Option<String>,
    pub reclaimed_bytes: u64,
}

pub struct DiskCleanupFinishCmd;

impl CommandHandler for DiskCleanupFinishCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: DiskCleanupFinishInput = serde_json::from_value(payload)
                .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?;
            finish_cleanup_action(
                tx,
                &input.action_id,
                &input.status,
                input.trashed_path.as_deref(),
                input.reclaimed_bytes,
                ctx.now_ms,
            )
            .await?;
            Ok(serde_json::json!({
                "action_id": input.action_id,
                "status": input.status,
                "trashed_path": input.trashed_path,
                "reclaimed_bytes": input.reclaimed_bytes,
            }))
        })
    }
}
