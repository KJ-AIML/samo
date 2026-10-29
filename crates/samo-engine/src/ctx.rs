use samo_blueprint::{Permission, Surface};
use samo_primitives::{ActorId, TenantId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub surface: Surface,
    pub permissions: Vec<Permission>,
}

pub struct CommandCtx {
    pub auth: AuthContext,
    pub now_ms: i64,
}
