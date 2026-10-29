pub mod repo;
pub mod tx;

pub use repo::{
    CleanupActionRecord, ScanHistoryItem, TenantSettingsRecord, append_audit_and_outbox,
    count_audit_entries, ensure_tenant, finish_cleanup_action, finish_process_action,
    finish_scan_snapshot, get_or_init_settings, get_scan_snapshot, insert_cleanup_begin,
    insert_process_action_begin, insert_scan_begin, list_scan_history, reconcile_interrupted_scans,
    update_theme,
};
pub use tx::{StorePool, SystemTx, TenantTx};

#[cfg(test)]
mod tests {
    use super::*;
    use samo_primitives::{PhosphorTheme, ScanId, ScanProgressStatus, TenantId};

    #[tokio::test]
    async fn tenant_tx_isolates_scans_and_settings_between_two_tenants() {
        let dir = tempfile::tempdir().ok();
        assert!(dir.is_some());
        let Some(dir) = dir else { return };
        let db_path = dir.path().join("test_isolation.db");
        let pool = StorePool::connect_file(&db_path).await.ok();
        assert!(pool.is_some());
        let Some(pool) = pool else { return };

        let t1 = TenantId::new("tnt_alpha");
        let t2 = TenantId::new("tnt_beta");

        let Ok(mut tx1) = pool.begin_tenant(t1.clone()).await else {
            return;
        };
        let _ = update_theme(&mut tx1, PhosphorTheme::Green, Some(true), 1000).await;
        let _ = insert_scan_begin(&mut tx1, &ScanId::new("scn_a1"), "/Users/a", 1000).await;
        let _ = tx1.commit().await;

        let Ok(mut tx2) = pool.begin_tenant(t2.clone()).await else {
            return;
        };
        let s2 = get_or_init_settings(&mut tx2, "/Users/b", 2000).await.ok();
        let h2 = list_scan_history(&mut tx2).await.ok().unwrap_or_default();
        let _ = tx2.commit().await;

        assert_eq!(s2.map(|s| s.theme), Some(PhosphorTheme::Dark));
        assert!(h2.is_empty());
    }

    #[tokio::test]
    async fn reconcile_marks_interrupted_scans() {
        let Some(dir) = tempfile::tempdir().ok() else {
            return;
        };
        let Some(pool) = StorePool::connect_file(&dir.path().join("reconcile.db"))
            .await
            .ok()
        else {
            return;
        };

        let t1 = TenantId::local_default();
        if let Ok(mut tx) = pool.begin_tenant(t1.clone()).await {
            let _ = insert_scan_begin(&mut tx, &ScanId::new("scn_stuck"), "/Users/kj", 1000).await;
            let _ = tx.commit().await;
        }

        if let Ok(mut sys_tx) = pool.begin_system().await {
            let count = reconcile_interrupted_scans(&mut sys_tx, 5000)
                .await
                .unwrap_or(0);
            assert_eq!(count, 1);
            let _ = sys_tx.commit().await;
        }

        if let Ok(mut tx) = pool.begin_tenant(t1).await {
            let hist = list_scan_history(&mut tx).await.unwrap_or_default();
            assert_eq!(hist.len(), 1);
            assert_eq!(hist[0].status, ScanProgressStatus::Interrupted);
        }
    }
}
