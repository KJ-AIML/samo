use samo_primitives::{ErrorCode, SamoError, SamoResult};
use samo_store::StorePool;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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

/// Writes `script` as an executable `/bin/sh` program named `name` in `dir`: a stand-in for an extension's
/// helper in tests (it can say hello, flood, crash or hang on demand).
pub fn fake_helper(dir: &Path, name: &str, script: &str) -> SamoResult<PathBuf> {
    let io = |e: std::io::Error| SamoError::new(ErrorCode::InternalError, e.to_string());
    fs::create_dir_all(dir).map_err(io)?;
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).map_err(io)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(io)?;
    Ok(path)
}
