use crate::ctx::AuthContext;
use samo_blueprint::CommandSpec;
use samo_primitives::{ErrorCode, SamoError, SamoResult};

pub struct AccessPolicy;

impl AccessPolicy {
    /// Default-deny access check: requires both the command's surface and permission.
    pub fn check(spec: &CommandSpec, auth: &AuthContext) -> SamoResult<()> {
        if !spec.surfaces.contains(&auth.surface) {
            return Err(SamoError::new(
                ErrorCode::PermissionDenied,
                format!(
                    "command `{}` is not permitted on surface `{:?}`",
                    spec.name, auth.surface
                ),
            ));
        }

        if !auth.permissions.contains(&spec.permission) {
            return Err(SamoError::new(
                ErrorCode::PermissionDenied,
                format!(
                    "actor `{}` lacks required permission `{:?}` for `{}`",
                    auth.actor_id.as_str(),
                    spec.permission,
                    spec.name
                ),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_blueprint::{Permission, Surface, find_command_spec};
    use samo_primitives::{ActorId, TenantId};

    #[test]
    fn default_deny_rejects_unlisted_surface_or_missing_permission() {
        let Some(spec) = find_command_spec("disk.scan.begin") else {
            return;
        };

        let missing_perm = AuthContext {
            tenant_id: TenantId::local_default(),
            actor_id: ActorId::local_owner(),
            surface: Surface::Web,
            permissions: vec![Permission::DiskRead],
        };
        assert!(AccessPolicy::check(spec, &missing_perm).is_err());

        let wrong_surface = AuthContext {
            tenant_id: TenantId::local_default(),
            actor_id: ActorId::local_owner(),
            surface: Surface::Internal,
            permissions: vec![Permission::DiskScan],
        };
        assert!(AccessPolicy::check(spec, &wrong_surface).is_err());

        let valid = AuthContext {
            tenant_id: TenantId::local_default(),
            actor_id: ActorId::local_owner(),
            surface: Surface::Web,
            permissions: vec![Permission::DiskScan],
        };
        assert!(AccessPolicy::check(spec, &valid).is_ok());
    }
}
