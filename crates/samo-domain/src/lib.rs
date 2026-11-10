pub mod cleanup;
pub mod disk;
pub mod extensions;
pub mod packing;
pub mod paths;
pub mod process;
pub mod process_groups;
pub mod store;
pub mod telemetry;

pub use cleanup::{is_protected_path_on, is_protected_system_path, validate_trash_target};
pub use disk::{
    RawDiskEntry, RawEntryKind, aggregate_scan_entries, classify_file_path, detect_project_marker,
    is_artifact_dir_name, validate_scan_config,
};
pub use packing::{PackInput, pack_circles};
pub use paths::{is_absolute_path, normalize_path, scan_root_on};
pub use process::{filter_and_sort_processes, verify_identity};
pub use process_groups::{
    GroupKey, ProcessGroup, app_bundle_of, app_name_of, app_name_of_on, app_of_on, group_processes,
    group_processes_on, row_matches, sort_groups, visible_members,
};
pub use telemetry::{TelemetryRingBuffer, clamp_sample_interval_ms};
