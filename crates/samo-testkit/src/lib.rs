pub mod http;
pub mod store;

use samo_primitives::{ErrorCode, SamoError, SamoResult};
use samo_store::StorePool;
use std::fs;
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

/// Lays out a stand-in for an extension's helper in `dir`, on every OS: a fresh copy of `exe` (the
/// `samo-fake-helper` program, `env!("CARGO_BIN_EXE_samo-fake-helper")` in samo-extensions' tests) named `name`
/// (`name.exe` on Windows), and `script` beside it as `name.script`, which it follows: it can say hello, flood,
/// crash, hang or spawn children on demand. A copy, not a hard link, which a package may not hold (R5).
pub fn fake_helper(exe: &Path, dir: &Path, name: &str, script: &str) -> SamoResult<PathBuf> {
    let io = |e: std::io::Error| SamoError::new(ErrorCode::InternalError, e.to_string());
    fs::create_dir_all(dir).map_err(io)?;
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    fs::copy(exe, &path).map_err(io)?;
    fs::write(dir.join(format!("{name}.script")), script).map_err(io)?;
    Ok(path)
}
