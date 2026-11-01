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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_server_can_be_read() {
        // This test runs inside a login session (a Mac, or GitHub's macOS runner): the list exists, and a process
        // that never opened a window owns none.
        let windows = SystemWindows;
        assert_eq!(windows.count(&[std::process::id()]), Some(0));
    }
}
