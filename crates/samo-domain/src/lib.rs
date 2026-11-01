pub mod cleanup;
pub mod disk;
pub mod extensions;
pub mod packing;
pub mod process;
pub mod store;
pub mod telemetry;

pub use cleanup::{is_protected_system_path, validate_trash_target};
pub use disk::{
    RawDiskEntry, RawEntryKind, aggregate_scan_entries, classify_file_path, detect_project_marker,
    is_artifact_dir_name, validate_scan_config,
};
pub use packing::{PackInput, pack_circles};
pub use process::{filter_and_sort_processes, verify_identity};
pub use telemetry::{TelemetryRingBuffer, clamp_sample_interval_ms};
