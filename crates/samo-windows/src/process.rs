//! R4 on Windows: Quit only after checking it is still the same process (PID and start time).

use samo_domain::verify_identity;
use samo_primitives::{ErrorCode, ProcessIdentity, SamoError, SamoResult};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use sysinfo::{Pid, ProcessesToUpdate, System};

/// `CREATE_NO_WINDOW`: no console window flashes up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

/// Windows' own `taskkill`, by its full path, as the Mac runs `/bin/kill`.
fn taskkill() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("taskkill.exe")
}

/// R4: verifies that `requested` `(pid, start_time_secs)` still matches the live process right before closing it
/// (`taskkill`, the TERM of Windows: it asks the app's windows to close) or ending it (`taskkill /F`, for
/// `SIGKILL`). Only that process: its children were never verified.
pub fn terminate_verified_process(requested: ProcessIdentity, signal: &str) -> SamoResult<()> {
    let live = inspect_process_identity(requested.pid);
    verify_identity(requested, live)?;

    let force = matches!(
        signal.to_ascii_uppercase().as_str(),
        "SIGKILL" | "KILL" | "9"
    );
    let mut command = Command::new(taskkill());
    command.arg("/PID").arg(requested.pid.to_string());
    if force {
        command.arg("/F");
    }
    let out = command
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| {
            SamoError::new(
                ErrorCode::InternalError,
                format!("failed to invoke taskkill: {e}"),
            )
        })?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(SamoError::new(
            ErrorCode::PermissionDenied,
            format!(
                "Windows refused to {} PID {}: {why}",
                if force { "end" } else { "close" },
                requested.pid
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::Duration;

    #[test]
    fn quits_only_the_verified_process() -> Result<(), String> {
        let mut child = Command::new("cmd")
            .args(["/C", "pause"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        let pid = child.id();
        let live = inspect_process_identity(pid).ok_or("no identity")?;
        let recycled = ProcessIdentity {
            pid,
            start_time_secs: live.start_time_secs + 1000,
        };

        let refused = terminate_verified_process(recycled, "SIGKILL").map_err(|e| e.code);
        let still_running = matches!(child.try_wait(), Ok(None));
        let ended = terminate_verified_process(live, "SIGKILL");
        let mut exited = false;
        for _ in 0..50 {
            if matches!(child.try_wait(), Ok(Some(_))) {
                exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !exited {
            let _ = child.kill();
        }

        assert_eq!(refused, Err(ErrorCode::ProcessIdentityMismatch));
        assert!(still_running, "a recycled PID must never be signalled");
        assert!(ended.is_ok(), "{ended:?}");
        assert!(exited, "the verified process should have ended");
        Ok(())
    }
}
