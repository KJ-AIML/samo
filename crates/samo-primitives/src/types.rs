use crate::reading::Reading;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Media,
    Documents,
    Code,
    Archives,
    Applications,
    System,
    Other,
}

impl FileCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Media => "Media",
            Self::Documents => "Documents",
            Self::Code => "Code",
            Self::Archives => "Archives",
            Self::Applications => "Applications",
            Self::System => "System",
            Self::Other => "Other",
        }
    }

    pub fn all() -> [Self; 7] {
        [
            Self::Media,
            Self::Documents,
            Self::Code,
            Self::Archives,
            Self::Applications,
            Self::System,
            Self::Other,
        ]
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhosphorTheme {
    #[default]
    Dark,
    Light,
    Green,
    Amber,
    Blue,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileNodeIdentity {
    pub device_id: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryBreakdownItem {
    pub category: FileCategory,
    pub label: String,
    pub bytes: u64,
    pub file_count: u64,
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanFolderEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    pub category: FileCategory,
    pub identity: FileNodeIdentity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanFileEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub category: FileCategory,
    pub identity: FileNodeIdentity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectArtifactItem {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub identity: FileNodeIdentity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub name: String,
    pub root_path: String,
    pub kind: String,
    pub total_bytes: u64,
    pub artifact_bytes: u64,
    pub source_bytes: u64,
    pub artifacts: Vec<ProjectArtifactItem>,
    pub identity: FileNodeIdentity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackedBall {
    pub id: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub category: FileCategory,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub identity: FileNodeIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanProgressStatus {
    Scanning,
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanSnapshot {
    pub scan_id: String,
    pub root_path: String,
    pub volume_name: String,
    pub status: ScanProgressStatus,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub volume_total_bytes: u64,
    pub volume_used_bytes: u64,
    pub volume_free_bytes: u64,
    pub scanned_bytes: u64,
    pub scanned_files: u64,
    pub unreadable_paths: u64,
    pub categories: Vec<CategoryBreakdownItem>,
    pub largest_folders: Vec<ScanFolderEntry>,
    pub largest_files: Vec<ScanFileEntry>,
    pub projects: Vec<ProjectSummary>,
    pub balls: Vec<PackedBall>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPressure {
    Normal,
    Elevated,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryBreakdown {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub wired_bytes: Reading<u64>,
    pub compressed_bytes: Reading<u64>,
    pub cached_bytes: Reading<u64>,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub pressure: MemoryPressure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskIoRates {
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
    pub total_read_bytes: u64,
    pub total_write_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkRates {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub primary_interface: String,
    pub local_ip: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyImpact {
    Low,
    Moderate,
    High,
    VeryHigh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessRow {
    pub identity: ProcessIdentity,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub executable_path: String,
    pub user: String,
    pub cpu_pct: f32,
    pub memory_bytes: u64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
    pub energy_impact: EnergyImpact,
    pub is_self: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSummary {
    pub hostname: String,
    pub os_version: String,
    pub chip_brand: String,
    pub physical_cores: u32,
    pub logical_cores: u32,
    pub perf_cores: Reading<u32>,
    pub eff_cores: Reading<u32>,
    pub uptime_secs: u64,
    pub boot_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryPoint {
    pub timestamp_ms: i64,
    pub generation: u64,
    pub cpu_overall_pct: Reading<f32>,
    pub cpu_perf_cores_pct: Reading<Vec<f32>>,
    pub cpu_eff_cores_pct: Reading<Vec<f32>>,
    pub memory: Reading<MemoryBreakdown>,
    pub disk_io: Reading<DiskIoRates>,
    pub network: Reading<NetworkRates>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub system: SystemSummary,
    pub latest: TelemetryPoint,
    pub history: Vec<TelemetryPoint>,
    pub processes: Vec<ProcessRow>,
    pub total_processes: usize,
    pub self_pid: u32,
    pub provider_healthy: bool,
}
