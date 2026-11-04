//! The list's shortcut (Win+Alt+V unless you choose another), held for the list's window while Clips is on. One
//! another app holds is refused and said so on the page, never forced (R18). Windows lets it go when Clips stops.

use crate::page::ShortcutState;
use crate::settings::Shortcut;
use winsafe::{self as w, co};

/// The shortcut's number with Windows.
pub const ID: i32 = 1;

const LETTERS: [co::VK; 26] = [
    co::VK::CHAR_A,
    co::VK::CHAR_B,
    co::VK::CHAR_C,
    co::VK::CHAR_D,
    co::VK::CHAR_E,
    co::VK::CHAR_F,
    co::VK::CHAR_G,
    co::VK::CHAR_H,
    co::VK::CHAR_I,
    co::VK::CHAR_J,
    co::VK::CHAR_K,
    co::VK::CHAR_L,
    co::VK::CHAR_M,
    co::VK::CHAR_N,
    co::VK::CHAR_O,
    co::VK::CHAR_P,
    co::VK::CHAR_Q,
    co::VK::CHAR_R,
    co::VK::CHAR_S,
    co::VK::CHAR_T,
    co::VK::CHAR_U,
    co::VK::CHAR_V,
    co::VK::CHAR_W,
    co::VK::CHAR_X,
    co::VK::CHAR_Y,
    co::VK::CHAR_Z,
];

const DIGITS: [co::VK; 10] = [
    co::VK::CHAR_0,
    co::VK::CHAR_1,
    co::VK::CHAR_2,
    co::VK::CHAR_3,
    co::VK::CHAR_4,
    co::VK::CHAR_5,
    co::VK::CHAR_6,
    co::VK::CHAR_7,
    co::VK::CHAR_8,
    co::VK::CHAR_9,
];

/// Windows' modifiers and key for a shortcut: a letter or a digit with at least one modifier, else `None`.
pub fn keys(s: &Shortcut) -> Option<(co::MOD, co::VK)> {
    let key = s.key.to_ascii_uppercase();
    let from = |first: char| usize::try_from(u32::from(key).checked_sub(u32::from(first))?).ok();
    let vk = match key {
        'A'..='Z' => LETTERS.get(from('A')?).copied()?,
        '0'..='9' => DIGITS.get(from('0')?).copied()?,
        _ => return None,
    };
    if !(s.ctrl || s.alt || s.win) {
        return None;
    }
    let mut mods = co::MOD::NOREPEAT;
    for (on, flag) in [
        (s.ctrl, co::MOD::CONTROL),
        (s.alt, co::MOD::ALT),
        (s.shift, co::MOD::SHIFT),
        (s.win, co::MOD::WIN),
    ] {
        if on {
            mods |= flag;
        }
    }
    Some((mods, vk))
}

/// Holds `shortcut` for `window` (or this thread, for `HWND::NULL`) in place of the one before; whether Windows
/// gave it. No shortcut holds nothing.
pub fn register(window: &w::HWND, shortcut: Option<Shortcut>) -> ShortcutState {
    let _ = window.UnregisterHotKey(ID);
    let Some(shortcut) = shortcut else {
        return ShortcutState::Ready;
    };
    match keys(&shortcut) {
        Some((mods, vk)) if window.RegisterHotKey(ID, mods, vk).is_ok() => ShortcutState::Ready,
        _ => ShortcutState::Refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use winsafe::prelude::*;

    fn shortcut(ctrl: bool, alt: bool, shift: bool, win: bool, key: char) -> Shortcut {
        Shortcut {
            ctrl,
            alt,
            shift,
            win,
            key,
        }
    }

    #[test]
    fn a_shortcut_is_windows_modifiers_and_key() {
        assert_eq!(
            keys(&Shortcut::standard()),
            Some((
                co::MOD::NOREPEAT | co::MOD::ALT | co::MOD::WIN,
                co::VK::CHAR_V
            ))
        );
        assert_eq!(
            keys(&shortcut(true, false, true, false, 'c')),
            Some((
                co::MOD::NOREPEAT | co::MOD::CONTROL | co::MOD::SHIFT,
                co::VK::CHAR_C
            ))
        );
        assert_eq!(
            keys(&shortcut(false, true, false, false, '7')),
            Some((co::MOD::NOREPEAT | co::MOD::ALT, co::VK::CHAR_7))
        );
        assert_eq!(keys(&shortcut(false, true, false, false, 'ก')), None);
        assert_eq!(
            keys(&shortcut(false, false, false, false, 'V')),
            None,
            "a bare V would steal typing"
        );
    }

    /// One another app holds is refused, never forced (R18): Windows gives a shortcut to one thread at a time.
    #[test]
    fn a_shortcut_another_holds_is_refused() {
        // Nobody uses this one.
        let rare = shortcut(true, true, true, true, 'Q');
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let other = thread::spawn(move || {
            let state = register(&w::HWND::NULL, Some(rare));
            let _ = held_tx.send(state);
            let _ = done_rx.recv();
            let _ = w::HWND::NULL.UnregisterHotKey(ID);
        });
        assert_eq!(held_rx.recv().ok(), Some(ShortcutState::Ready));
        assert_eq!(register(&w::HWND::NULL, Some(rare)), ShortcutState::Refused);
        assert_eq!(
            register(&w::HWND::NULL, None),
            ShortcutState::Ready,
            "no shortcut takes nothing"
        );
        let _ = done_tx.send(());
        let _ = other.join();
    }
}
