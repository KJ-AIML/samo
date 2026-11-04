//! Pasting where you were, on Windows (ADR-005 C3, R24): the paste flow's window is Windows' own, and Ctrl+V is
//! `SendInput`. A window whose program runs as administrator, when Clips doesn't, can't be typed into, so Clips
//! stops at the clipboard; so it does when Windows won't say.

use crate::paste::Windows;
use std::time::Duration;
use winsafe::{self as w, co};

/// This Windows session's windows and keyboard.
pub struct WinWindows;

/// Whether the process runs elevated; `None` if Windows won't say.
pub fn elevated(pid: u32) -> Option<bool> {
    let process =
        w::HPROCESS::OpenProcess(co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let token = process.OpenProcessToken(co::TOKEN::QUERY).ok()?;
    match token
        .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::Elevation)
        .ok()?
    {
        w::TokenInfo::Elevation(e) => Some(e.TokenIsElevated()),
        _ => None,
    }
}

fn key(vk: co::VK, up: bool) -> w::HwKbMouse {
    let mut input = w::KEYBDINPUT {
        wVk: vk,
        // The scan code too: some remote and virtual-machine consoles read only that.
        wScan: u16::try_from(w::MapVirtualKey(u32::from(vk.raw()), co::MAPVK::VK_TO_VSC))
            .unwrap_or(0),
        ..Default::default()
    };
    if up {
        input.dwFlags = co::KEYEVENTF::KEYUP;
    }
    w::HwKbMouse::Kb(input)
}

impl Windows for WinWindows {
    type Window = w::HWND;

    fn front(&self) -> Option<w::HWND> {
        w::HWND::GetForegroundWindow()
    }

    fn activate(&self, target: &w::HWND) {
        let _ = target.SetForegroundWindow();
    }

    fn elevated(&self, target: &w::HWND) -> bool {
        if elevated(std::process::id()) == Some(true) {
            return false;
        }
        let (_, pid) = target.GetWindowThreadProcessId();
        elevated(pid).unwrap_or(true)
    }

    fn keys_held(&self) -> bool {
        [co::VK::MENU, co::VK::SHIFT, co::VK::LWIN, co::VK::RWIN]
            .into_iter()
            .any(w::GetAsyncKeyState)
    }

    fn press_paste(&self) -> bool {
        let keys = [
            key(co::VK::CONTROL, false),
            key(co::VK::CHAR_V, false),
            key(co::VK::CHAR_V, true),
            key(co::VK::CONTROL, true),
        ];
        // Windows says how many it took; none means it blocked them (a higher-integrity window, the secure desktop).
        w::SendInput(&keys).is_ok_and(|taken| taken == 4)
    }

    fn sleep(&self, time: Duration) {
        std::thread::sleep(time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test itself runs as you; Windows says so (never pressing a key here).
    #[test]
    fn windows_says_whether_a_process_is_elevated() {
        assert!(elevated(std::process::id()).is_some());
        assert_eq!(elevated(u32::MAX), None, "no such process");
    }
}
