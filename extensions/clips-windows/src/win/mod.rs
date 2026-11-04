//! Clips' Windows side: the clipboard, its listener and the app in front (PR E), then the list, the shortcut and
//! pasting (PR F).

pub mod board;
pub mod hotkey;
pub mod listen;

/// The real clipboard in tests.
#[cfg(test)]
pub(crate) mod real {
    use crate::watcher::{Board, Put};
    use crate::win::board::WinBoard;
    use std::sync::{Mutex, MutexGuard};

    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// The real clipboard, for one test at a time; what was on it (as text) goes back after. These tests replace
    /// what you copied, so they run on CI, or when `SAMO_TEST_CLIPBOARD=1`.
    pub struct RealClipboard {
        before: Option<String>,
        _one: MutexGuard<'static, ()>,
    }

    impl Drop for RealClipboard {
        fn drop(&mut self) {
            if let Some(text) = self.before.take() {
                WinBoard::default().put(&Put {
                    text: Some(text),
                    ..Put::default()
                });
            }
        }
    }

    pub fn real_clipboard() -> Option<RealClipboard> {
        let allowed =
            std::env::var_os("CI").is_some() || std::env::var_os("SAMO_TEST_CLIPBOARD").is_some();
        if !allowed {
            eprintln!("skipped: the real clipboard tests run on CI, or with SAMO_TEST_CLIPBOARD=1");
            return None;
        }
        let one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(RealClipboard {
            before: WinBoard::default().text(),
            _one: one,
        })
    }
}
