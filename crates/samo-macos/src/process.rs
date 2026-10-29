use samo_domain::verify_identity;
use samo_primitives::{ErrorCode, ProcessIdentity, SamoError, SamoResult};
use std::process::Command;
use sysinfo::{Pid, ProcessesToUpdate, System};

pub fn inspect_process_identity(pid: u32) -> Option<ProcessIdentity> {
    let mut sys = System::new();
    let target_pid = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[target_pid]), true);
    let proc_info = sys.process(target_pid)?;
    Some(ProcessIdentity {
        pid,
        start_time_secs: proc_info.start_time(),
    })
}

/// R4: Verifies that `requested` `(pid, start_time_secs)` still matches the live OS process
/// right before sending `SIGTERM` or `SIGKILL`.
pub fn terminate_verified_process(requested: ProcessIdentity, signal: &str) -> SamoResult<()> {
    let live = inspect_process_identity(requested.pid);
    verify_identity(requested, live)?;

    let flag = match signal.to_ascii_uppercase().as_str() {
        "SIGKILL" | "KILL" | "9" => "-KILL",
        _ => "-TERM",
    };

    let status = Command::new("/bin/kill")
        .arg(flag)
        .arg(requested.pid.to_string())
        .status()
        .map_err(|e| {
            SamoError::new(
                ErrorCode::InternalError,
                format!("failed to invoke /bin/kill: {e}"),
            )
        })?;

    if !status.success() {
        return Err(SamoError::new(
            ErrorCode::PermissionDenied,
            format!("OS rejected signal `{flag}` for PID {}", requested.pid),
        ));
    }
    Ok(())
}
