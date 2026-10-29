use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    PermissionDenied,
    InvalidInput,
    NotFound,
    ProtectedPathDenied,
    InodeIdentityMismatch,
    ProcessIdentityMismatch,
    ScanAlreadyRunning,
    InternalError,
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[error("[{code:?}] {message}")]
pub struct SamoError {
    pub code: ErrorCode,
    pub message: String,
}

impl SamoError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub type SamoResult<T> = Result<T, SamoError>;
