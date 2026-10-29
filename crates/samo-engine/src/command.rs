use crate::ctx::CommandCtx;
use samo_primitives::SamoResult;
use samo_store::TenantTx;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type BoxCommandFuture<'a> = Pin<Box<dyn Future<Output = SamoResult<Value>> + Send + 'a>>;

pub trait CommandHandler: Send + Sync {
    fn handle<'a>(
        &'a self,
        ctx: &'a CommandCtx,
        tx: &'a mut TenantTx<'_>,
        payload: Value,
    ) -> BoxCommandFuture<'a>;
}
