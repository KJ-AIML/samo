use crate::command::{BoxCommandFuture, CommandHandler};
use crate::ctx::CommandCtx;
use samo_primitives::{ErrorCode, PhosphorTheme, SamoError};
use samo_store::{TenantTx, get_or_init_settings, update_theme};
use serde::Deserialize;
use serde_json::Value;

pub struct SettingsGetCmd;

impl CommandHandler for SettingsGetCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        _payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let rec = get_or_init_settings(tx, "/", ctx.now_ms).await?;
            serde_json::to_value(rec)
                .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct SettingsThemeSetInput {
    pub theme: PhosphorTheme,
    pub include_self_process: Option<bool>,
}

pub struct SettingsThemeSetCmd;

impl CommandHandler for SettingsThemeSetCmd {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a> {
        Box::pin(async move {
            let input: SettingsThemeSetInput = serde_json::from_value(payload)
                .map_err(|e| SamoError::new(ErrorCode::InvalidInput, e.to_string()))?;
            let updated =
                update_theme(tx, input.theme, input.include_self_process, ctx.now_ms).await?;
            serde_json::to_value(updated)
                .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
        })
    }
}
