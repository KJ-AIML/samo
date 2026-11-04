//! Windows tells Clips when the clipboard changes (`WM_CLIPBOARDUPDATE`, through `clipboard-win`'s `Monitor`), so
//! Clips looks then: between copies it sleeps (it wakes only when its page would change: a pause ending, midnight,
//! at most hourly) (R21), a copy is kept at once, and two copies close together are both seen.

use crate::serve::Event;
use clipboard_win::Monitor;
use std::sync::mpsc::{self, Sender};
use std::thread;

/// Starts the listener's thread, which says `Changed` on `tx` at each change until Clips stops. False if Windows
/// wouldn't add the listener; Clips then looks twice a second instead.
pub fn listen(tx: Sender<Event>) -> bool {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let started = thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || {
            // The listener's window belongs to this thread, so it is made here.
            let Ok(mut monitor) = Monitor::new() else {
                let _ = ready_tx.send(false);
                return;
            };
            let _ = ready_tx.send(true);
            while matches!(monitor.recv(), Ok(true)) {
                if tx.send(Event::Changed).is_err() {
                    return;
                }
            }
        });
    started.is_ok() && ready_rx.recv().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::{Board, Put};
    use crate::win::board::WinBoard;
    use crate::win::real::real_clipboard;
    use std::time::Duration;

    #[test]
    fn windows_says_when_the_clipboard_changes() {
        let Some(_board) = real_clipboard() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        assert!(listen(tx), "the listener didn't start");
        WinBoard::default().put(&Put {
            text: Some("listen test".into()),
            ..Put::default()
        });
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)),
            Ok(Event::Changed)
        ));
        // One copy may be several updates; then nothing until the next.
        while rx.recv_timeout(Duration::from_millis(100)).is_ok() {}
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
