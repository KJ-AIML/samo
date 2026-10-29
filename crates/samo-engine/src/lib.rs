pub mod access;
pub mod auth;
pub mod command;
pub mod commands;
pub mod ctx;
pub mod engine;
pub mod registry;

pub use auth::{AuthService, DEFAULT_LOCAL_CLI_TOKEN, DEFAULT_LOCAL_WEB_TOKEN};
pub use engine::Engine;
pub use registry::CommandRegistry;

#[cfg(test)]
mod tests {
    use super::*;
    use samo_blueprint::Surface;
    use samo_primitives::{ErrorCode, FileNodeIdentity, PhosphorTheme, ProcessIdentity, TenantId};
    use samo_store::count_audit_entries;
    use samo_testkit::TestDb;

    #[tokio::test]
    async fn execute_writes_audit_log_on_mutation_and_rolls_back_on_error() {
        let Ok(test_db) = TestDb::new().await else {
            return;
        };
        let engine = Engine::new(test_db.pool.clone());

        let res = engine
            .execute(
                DEFAULT_LOCAL_WEB_TOKEN,
                Surface::Web,
                "settings.theme.set",
                serde_json::json!({ "theme": PhosphorTheme::Green, "include_self_process": true }),
            )
            .await;
        assert!(res.is_ok());

        let Ok(mut tx) = test_db.pool.begin_tenant(TenantId::local_default()).await else {
            return;
        };
        let count = count_audit_entries(&mut tx).await.unwrap_or(0);
        assert_eq!(count, 1);

        // Protected path cleanup fails R2 and does not increment audit log
        let bad_cleanup = engine
            .execute(
                DEFAULT_LOCAL_WEB_TOKEN,
                Surface::Web,
                "disk.cleanup.begin",
                serde_json::json!({
                    "scan_id": "scn_1",
                    "target_path": "/System",
                    "scanned_identity": FileNodeIdentity { device_id: 1, inode: 1 },
                    "live_identity": FileNodeIdentity { device_id: 1, inode: 1 },
                }),
            )
            .await;
        assert_eq!(
            bad_cleanup.map_err(|e| e.code),
            Err(ErrorCode::ProtectedPathDenied)
        );

        let Ok(mut tx2) = test_db.pool.begin_tenant(TenantId::local_default()).await else {
            return;
        };
        let count_after = count_audit_entries(&mut tx2).await.unwrap_or(0);
        assert_eq!(count_after, 1);
    }

    #[tokio::test]
    async fn terminate_rejects_recycled_pid_with_different_start_time() {
        let Ok(test_db) = TestDb::new().await else {
            return;
        };
        let engine = Engine::new(test_db.pool.clone());

        let res = engine
            .execute(
                DEFAULT_LOCAL_WEB_TOKEN,
                Surface::Web,
                "process.terminate.begin",
                serde_json::json!({
                    "requested_identity": ProcessIdentity { pid: 9001, start_time_secs: 1000 },
                    "live_identity": ProcessIdentity { pid: 9001, start_time_secs: 2000 },
                    "signal": "SIGTERM"
                }),
            )
            .await;
        assert_eq!(
            res.map_err(|e| e.code),
            Err(ErrorCode::ProcessIdentityMismatch)
        );
    }
}
