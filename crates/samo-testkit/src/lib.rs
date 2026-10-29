use samo_primitives::{ErrorCode, SamoError, SamoResult};
use samo_store::StorePool;
use tempfile::TempDir;

pub struct TestDb {
    pub dir: TempDir,
    pub pool: StorePool,
}

impl TestDb {
    pub async fn new() -> SamoResult<Self> {
        let dir = tempfile::tempdir()
            .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()))?;
        let db_path = dir.path().join("samo_test.db");
        let pool = StorePool::connect_file(&db_path).await?;
        Ok(Self { dir, pool })
    }
}
