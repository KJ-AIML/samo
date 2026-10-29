use samo_primitives::{ErrorCode, SamoError, SamoResult, TenantId};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::path::Path;
use std::str::FromStr;

#[derive(Clone)]
pub struct StorePool {
    pool: SqlitePool,
}

impl StorePool {
    pub async fn connect_file(path: &Path) -> SamoResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SamoError::new(
                    ErrorCode::InternalError,
                    format!("failed to create db directory: {e}"),
                )
            })?;
        }
        let url = format!("sqlite://{}", path.to_string_lossy());
        let opts = SqliteConnectOptions::from_str(&url)
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(10));

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;

        let store = Self { pool };
        store.run_migrations().await?;
        Ok(store)
    }

    pub async fn run_migrations(&self) -> SamoResult<()> {
        let m1 = include_str!("../migrations/0001_directory_and_settings.sql");
        let m2 = include_str!("../migrations/0002_scans_and_snapshots.sql");
        let m3 = include_str!("../migrations/0003_audit_and_outbox.sql");

        for sql in [m1, m2, m3] {
            sqlx::raw_sql(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
        }
        Ok(())
    }

    pub async fn begin_tenant(&self, tenant_id: TenantId) -> SamoResult<TenantTx<'_>> {
        let tx = self
            .pool
            .begin()
            .await
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
        Ok(TenantTx { tenant_id, tx })
    }

    pub async fn begin_system(&self) -> SamoResult<SystemTx<'_>> {
        let tx = self
            .pool
            .begin()
            .await
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
        Ok(SystemTx { tx })
    }
}

/// TenantTx binds `tenant_id` from authentication and keeps `tx` strictly private.
pub struct TenantTx<'c> {
    tenant_id: TenantId,
    tx: Transaction<'c, Sqlite>,
}

impl<'c> TenantTx<'c> {
    pub fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    pub(crate) fn conn(&mut self) -> &mut sqlx::SqliteConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> SamoResult<()> {
        self.tx
            .commit()
            .await
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
    }
}

pub struct SystemTx<'c> {
    tx: Transaction<'c, Sqlite>,
}

impl<'c> SystemTx<'c> {
    pub(crate) fn conn(&mut self) -> &mut sqlx::SqliteConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> SamoResult<()> {
        self.tx
            .commit()
            .await
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))
    }
}
