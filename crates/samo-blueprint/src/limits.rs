use serde::{Deserialize, Serialize};

pub const MAX_SCAN_THREADS: usize = 4;
pub const DEFAULT_SCAN_THREADS: usize = 4;
pub const MAX_TOP_FILES: usize = 500;
pub const MAX_TOP_FOLDERS: usize = 100;
pub const MAX_RING_BUFFER_SAMPLES: usize = 3600;
pub const MIN_SAMPLE_INTERVAL_MS: u64 = 250;
pub const MAX_SAMPLE_INTERVAL_MS: u64 = 10_000;
pub const DEFAULT_SAMPLE_INTERVAL_MS: u64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanConfig {
    pub max_threads: usize,
    pub top_files_limit: usize,
    pub top_folders_limit: usize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_threads: DEFAULT_SCAN_THREADS,
            top_files_limit: 100,
            top_folders_limit: 50,
        }
    }
}
