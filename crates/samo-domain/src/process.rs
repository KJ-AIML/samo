use samo_primitives::{ErrorCode, ProcessIdentity, ProcessRow, SamoError, SamoResult};

/// R4: Pairs PID with process creation timestamp (`start_time_secs`) so a recycled PID
/// cannot redirect a signal to a newly spawned process.
pub fn verify_identity(
    requested: ProcessIdentity,
    live: Option<ProcessIdentity>,
) -> SamoResult<()> {
    let Some(live_id) = live else {
        return Err(SamoError::new(
            ErrorCode::NotFound,
            format!("process PID {} is no longer running", requested.pid),
        ));
    };

    if requested.pid != live_id.pid || requested.start_time_secs != live_id.start_time_secs {
        return Err(SamoError::new(
            ErrorCode::ProcessIdentityMismatch,
            format!(
                "R4 process identity mismatch for PID {}: requested start_time_secs {} != live {}",
                requested.pid, requested.start_time_secs, live_id.start_time_secs
            ),
        ));
    }

    Ok(())
}

pub fn filter_and_sort_processes(
    rows: &[ProcessRow],
    include_self: bool,
    search_query: &str,
    sort_by: &str,
) -> Vec<ProcessRow> {
    let query_lower = search_query.trim().to_ascii_lowercase();
    let mut filtered: Vec<ProcessRow> = rows
        .iter()
        .filter(|r| {
            if !include_self && r.is_self {
                return false;
            }
            if query_lower.is_empty() {
                return true;
            }
            r.name.to_ascii_lowercase().contains(&query_lower)
                || r.executable_path
                    .to_ascii_lowercase()
                    .contains(&query_lower)
                || r.identity.pid.to_string().contains(&query_lower)
        })
        .cloned()
        .collect();

    match sort_by {
        "memory" => filtered.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes)),
        "disk" => filtered.sort_by(|a, b| {
            (b.disk_read_bytes_per_sec + b.disk_write_bytes_per_sec)
                .cmp(&(a.disk_read_bytes_per_sec + a.disk_write_bytes_per_sec))
        }),
        "pid" => filtered.sort_by_key(|a| a.identity.pid),
        "name" => filtered.sort_by_key(|a| a.name.to_ascii_lowercase()),
        _ => filtered.sort_by(|a, b| b.cpu_pct.total_cmp(&a.cpu_pct)),
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_identity_rejects_recycled_pid_with_new_start_time() {
        let req = ProcessIdentity {
            pid: 4321,
            start_time_secs: 1_700_000_000,
        };
        let recycled = ProcessIdentity {
            pid: 4321,
            start_time_secs: 1_700_000_005,
        };
        let err = verify_identity(req, Some(recycled));
        assert_eq!(
            err.map_err(|e| e.code),
            Err(ErrorCode::ProcessIdentityMismatch)
        );
        assert!(verify_identity(req, Some(req)).is_ok());
    }
}
