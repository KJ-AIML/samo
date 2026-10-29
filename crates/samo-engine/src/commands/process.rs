use crate::command::{BoxCommandFuture, CommandHandler};
use crate::ctx::CommandCtx;
use samo_domain::verify_identity;
use samo_primitives::{ErrorCode, ProcessIdentity, SamoError};
use samo_store::{TenantTx, finish_process_action, insert_process_action_begin};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct ProcessTerminateBeginInput {
    pub requested_identity: ProcessIdentity,
    pub live_identity: Option<ProcessIdentity>,
    pub signal: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessTerminateBeginOutput {
    pub action_id: String,
    pub pid: u32,
    pub signal: String,
}

pub struct ProcessTerminateBeginCmd;

impl CommandHandler for ProcessTerminateBeginCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: ProcessTerminateBeginInput = serde_json::from_value(payload)
                .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?;

            // R4: Verify PID + start_time_secs identity before permitting any signal
            verify_identity(input.requested_identity, input.live_identity)?;

            let action_id = format!("prc_{}", uuid::Uuid::new_v4().simple());
            insert_process_action_begin(
                tx,
                &action_id,
                input.requested_identity,
                &input.signal,
                ctx.now_ms,
            )
            .await?;

            serde_json::to_value(ProcessTerminateBeginOutput {
                action_id,
                pid: input.requested_identity.pid,
                signal: input.signal,
            })
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct ProcessTerminateFinishInput {
    pub action_id: String,
    pub status: String,
}

pub struct ProcessTerminateFinishCmd;

impl CommandHandler for ProcessTerminateFinishCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: ProcessTerminateFinishInput = serde_json::from_value(payload)
                .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?;
            finish_process_action(tx, &input.action_id, &input.status, ctx.now_ms).await?;
            Ok(serde_json::json!({
                "action_id": input.action_id,
                "status": input.status,
            }))
        })
    }
}
