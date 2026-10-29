use crate::access::AccessPolicy;
use crate::auth::AuthService;
use crate::ctx::CommandCtx;
use crate::registry::CommandRegistry;
use futures::FutureExt;
use samo_blueprint::Surface;
use samo_primitives::{ErrorCode, SamoError, SamoResult};
use samo_store::{StorePool, append_audit_and_outbox};
use serde_json::Value;
use std::panic::AssertUnwindSafe;

#[derive(Clone)]
pub struct Engine {
    store: StorePool,
    registry: CommandRegistry,
    auth: AuthService,
}

impl Engine {
    pub fn new(store: StorePool) -> Self {
        Self {
            store,
            registry: CommandRegistry::new(),
            auth: AuthService::new(),
        }
    }

    pub fn store(&self) -> &StorePool {
        &self.store
    }

    /// Fixed Talvyn Engine execution pipeline:
    /// 1. Authenticate token -> TenantId comes strictly from auth, never from params
    /// 2. Lookup CommandSpec & run default-deny AccessPolicy check
    /// 3. Open TenantTx
    /// 4. Run handler inside `catch_unwind`
    /// 5. Write audit log + outbox event if command mutates state
    /// 6. Commit TenantTx
    pub async fn execute(
        &self,
        raw_token: &str,
        surface: Surface,
        command_name: &str,
        payload: Value,
    ) -> SamoResult<Value> {
        let auth_ctx = self.auth.authenticate(raw_token, surface)?;
        let Some(registered) = self.registry.get(command_name) else {
            return Err(SamoError::new(
                ErrorCode::NotFound,
                format!("unknown command `{command_name}`"),
            ));
        };

        AccessPolicy::check(registered.spec, &auth_ctx)?;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let ctx = CommandCtx {
            auth: auth_ctx.clone(),
            now_ms,
        };

        let mut tx = self.store.begin_tenant(auth_ctx.tenant_id.clone()).await?;
        let redacted_payload = redact_payload(&payload);

        let handler = registered.handler.clone();
        let unwind_res = AssertUnwindSafe(handler.handle(&ctx, &mut tx, payload))
            .catch_unwind()
            .await;

        let output = match unwind_res {
            Ok(Ok(val)) => val,
            Ok(Err(err)) => return Err(err),
            Err(_) => {
                return Err(SamoError::new(
                    ErrorCode::InternalError,
                    format!("command `{command_name}` caught unexpected unwind"),
                ));
            }
        };

        if registered.spec.mutates_state {
            let surface_label = format!("{surface:?}").to_ascii_lowercase();
            append_audit_and_outbox(
                &mut tx,
                &auth_ctx.actor_id,
                &surface_label,
                command_name,
                &redacted_payload,
                now_ms,
            )
            .await?;
        }

        tx.commit().await?;
        Ok(output)
    }
}

fn redact_payload(payload: &Value) -> String {
    let mut cloned = payload.clone();
    if let Some(obj) = cloned.as_object_mut() {
        for key in ["token", "secret", "password"] {
            if obj.contains_key(key) {
                obj.insert(key.to_string(), Value::String("[REDACTED]".to_string()));
            }
        }
        // Keep snapshot audit compact
        if obj.contains_key("snapshot") {
            obj.insert(
                "snapshot".to_string(),
                Value::String("[SCAN_SNAPSHOT_SUMMARY]".to_string()),
            );
        }
    }
    serde_json::to_string(&cloned).unwrap_or_else(|_| "{}".to_string())
}
