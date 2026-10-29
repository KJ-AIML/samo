pub mod error;
pub mod ids;
pub mod reading;
pub mod types;

pub use error::{ErrorCode, SamoError, SamoResult};
pub use ids::{ActorId, AuditId, ScanId, SnapshotId, TenantId};
pub use reading::{Reading, UnsupportedReason};
pub use types::{
    DiskIoRates, EnergyImpact, FileCategory, FileNodeIdentity, MemoryBreakdown, MemoryPressure,
    NetworkRates, PackedBall, PhosphorTheme, ProcessIdentity, ProcessRow, ProjectArtifactItem,
    ProjectSummary, ScanFileEntry, ScanFolderEntry, ScanProgressStatus, ScanSnapshot,
    SystemSummary, TelemetryPoint, TelemetrySnapshot,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_serializes_availability_states_without_fabricating_zero() {
        let avail: Reading<u64> = Reading::available(42, 1000);
        let unsup: Reading<u64> = Reading::unsupported(UnsupportedReason::SensorUnavailable);
        let stopped: Reading<u64> = Reading::stopped(2000);

        let avail_json = serde_json::to_string(&avail).unwrap_or_default();
        let unsup_json = serde_json::to_string(&unsup).unwrap_or_default();
        let stopped_json = serde_json::to_string(&stopped).unwrap_or_default();

        assert!(avail_json.contains("\"state\":\"available\""));
        assert!(avail_json.contains("\"value\":42"));
        assert!(unsup_json.contains("\"state\":\"unsupported\""));
        assert!(!unsup_json.contains("\"value\":0"));
        assert!(stopped_json.contains("\"state\":\"stopped\""));
    }

    #[test]
    fn token_prefix_validation() {
        assert!(ids::is_valid_token(
            "samo_loc_0123456789abcdef0123456789abcdef"
        ));
        assert!(ids::is_valid_token(
            "samo_cli_0123456789abcdef0123456789abcdef"
        ));
        assert!(!ids::is_valid_token("bad_prefix_1234"));
    }
}
