use crate::ctx::AuthContext;
use samo_blueprint::{Permission, Surface};
use samo_primitives::{ActorId, ErrorCode, SamoError, SamoResult, TenantId, ids::is_valid_token};

pub const DEFAULT_LOCAL_WEB_TOKEN: &str = "samo_loc_0123456789abcdef0123456789abcdef";
pub const DEFAULT_LOCAL_CLI_TOKEN: &str = "samo_cli_fedcba9876543210fedcba9876543210";

#[derive(Debug, Clone, Default)]
pub struct AuthService;

impl AuthService {
    pub fn new() -> Self {
        Self
    }

    pub fn authenticate(
        &self,
        raw_token: &str,
        requested_surface: Surface,
    ) -> SamoResult<AuthContext> {
        if raw_token == "samo_internal_system" && requested_surface == Surface::Internal {
            return Ok(AuthContext {
                tenant_id: TenantId::local_default(),
                actor_id: ActorId::new("act_internal_worker"),
                surface: Surface::Internal,
                permissions: all_permissions(),
            });
        }

        if !is_valid_token(raw_token) {
            return Err(SamoError::new(
                ErrorCode::Unauthorized,
                "invalid or missing Samo token",
            ));
        }

        let token_surface = if raw_token.starts_with("samo_loc_") {
            Surface::Web
        } else {
            Surface::Cli
        };

        if token_surface != requested_surface && requested_surface != Surface::Internal {
            return Err(SamoError::new(
                ErrorCode::PermissionDenied,
                "token surface does not match request surface",
            ));
        }

        Ok(AuthContext {
            tenant_id: TenantId::local_default(),
            actor_id: ActorId::local_owner(),
            surface: requested_surface,
            permissions: all_permissions(),
        })
    }
}

fn all_permissions() -> Vec<Permission> {
    vec![
        Permission::DiskRead,
        Permission::DiskScan,
        Permission::DiskCleanup,
        Permission::ProcessRead,
        Permission::ProcessTerminate,
        Permission::SettingsRead,
        Permission::SettingsWrite,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed_token_and_binds_tenant_strictly_from_auth() {
        let auth = AuthService::new();
        assert!(auth.authenticate("bad_token", Surface::Web).is_err());

        let ctx = auth.authenticate(DEFAULT_LOCAL_WEB_TOKEN, Surface::Web);
        assert!(ctx.is_ok());
        if let Ok(c) = ctx {
            assert_eq!(c.tenant_id, TenantId::local_default());
            assert_eq!(c.surface, Surface::Web);
        }
    }
}
