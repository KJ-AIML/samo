//! Which processes own windows (ADR-004, R15). An extension may open windows only if its manifest declares them;
//! the supervisor asks this every 2 s for the helper's whole process tree.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Counts windows by owner. A trait so tests can stand in for the window server.
pub trait WindowOwners: Send + Sync {
    /// How many windows, on screen or not, on any Space, the processes `pids` own. `None` if it can't be told.
    fn count(&self, pids: &[u32]) -> Option<usize>;
}

/// The window server, through `CGWindowListCopyWindowInfo`, read with checked casts (no `unsafe`).
pub struct SystemWindows;

impl SystemWindows {
    /// Neither a Mac nor Windows: it can't be told (ADR-006 R28).
    #[cfg(not(any(target_os = "macos", windows)))]
    fn owners() -> Option<HashMap<u32, usize>> {
        None
    }

    /// On Windows: every visible top-level window, by the process that owns it (ADR-006). A hidden window is left
    /// out: Windows gives console programs a hidden console host, and a window shown later is seen at the next
    /// check, 2 s on.
    #[cfg(windows)]
    fn owners() -> Option<HashMap<u32, usize>> {
        let mut owners = HashMap::new();
        winsafe::EnumWindows(|hwnd: winsafe::HWND| {
            if hwnd.IsWindowVisible() {
                let (_, pid) = hwnd.GetWindowThreadProcessId();
                *owners.entry(pid).or_insert(0) += 1;
            }
            true
        })
        .ok()?;
        Some(owners)
    }

    #[cfg(target_os = "macos")]
    fn owners() -> Option<HashMap<u32, usize>> {
        use objc2::runtime::AnyObject;
        use objc2_core_graphics::{
            CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID,
        };
        use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

        let list = CGWindowListCopyWindowInfo(CGWindowListOption::OptionAll, kCGNullWindowID)?;
        let any: &AnyObject = list.as_ref();
        let array = any.downcast_ref::<NSArray>()?;
        let key = NSString::from_str("kCGWindowOwnerPID");
        let mut owners = HashMap::new();
        for item in array.iter() {
            let Some(dict) = item.downcast_ref::<NSDictionary>() else {
                continue;
            };
            let Some(value) = dict.objectForKey(&key) else {
                continue;
            };
            let Some(pid) = value
                .downcast_ref::<NSNumber>()
                .and_then(|n| u32::try_from(n.integerValue()).ok())
            else {
                continue;
            };
            *owners.entry(pid).or_insert(0) += 1;
        }
        Some(owners)
    }
}

impl WindowOwners for SystemWindows {
    fn count(&self, pids: &[u32]) -> Option<usize> {
        let owners = Self::owners()?;
        Some(pids.iter().filter_map(|p| owners.get(p)).sum())
    }
}

/// The window check a host uses.
#[derive(Clone)]
pub struct WindowCheck(pub Arc<dyn WindowOwners>);

impl Default for WindowCheck {
    fn default() -> Self {
        Self(Arc::new(SystemWindows))
    }
}

impl fmt::Debug for WindowCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WindowCheck")
    }
}

#[cfg(all(test, any(target_os = "macos", windows)))]
mod tests {
    use super::*;

    #[test]
    fn the_window_server_can_be_read() {
        // This test runs inside a login session (a Mac or a Windows PC, or GitHub's runners): the list exists, and
        // a process that never opened a window owns none.
        let windows = SystemWindows;
        assert_eq!(windows.count(&[std::process::id()]), Some(0));
    }

    /// Opens a real window on the desktop, so it is run by hand: `cargo test -p samo-extensions -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "opens a window on the desktop"]
    fn a_visible_window_is_counted_for_its_owner() -> Result<(), String> {
        use std::time::{Duration, Instant};
        let root = std::env::var("SystemRoot").map_err(|e| e.to_string())?;
        let mut shown = std::process::Command::new(format!(
            r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe"
        ))
        .args([
            "-NoProfile",
            "-Command",
            "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show('Samo test')",
        ])
        .spawn()
        .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut seen = SystemWindows.count(&[shown.id()]);
        while seen == Some(0) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            seen = SystemWindows.count(&[shown.id()]);
        }
        let _ = shown.kill();
        let _ = shown.wait();
        assert!(seen.is_some_and(|n| n >= 1), "{seen:?}");
        Ok(())
    }
}
