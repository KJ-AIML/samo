//! Waiting on Windows for the screen, with a limit. If a capture never answers, Shots must not stay busy (every
//! shortcut would be ignored until it restarted): after the limit it gives up and says so, and an answer that arrives
//! later is ignored. Ported from the Mac's `Watchdog.swift`.

use std::time::Duration;

/// How long a capture may take.
pub const LIMIT: Duration = Duration::from_secs(8);

#[derive(Debug, Default)]
pub struct Watchdog {
    current: u64,
    waiting: bool,
}

impl Watchdog {
    /// A wait begins; its token.
    pub fn begin(&mut self) -> u64 {
        self.current += 1;
        self.waiting = true;
        self.current
    }

    /// The answer for `token` arrived. False if too late (the wait was given up, or another began): ignore it.
    pub fn answered(&mut self, token: u64) -> bool {
        if token != self.current || !self.waiting {
            return false;
        }
        self.waiting = false;
        true
    }

    /// The limit passed for `token`. True if it was still waiting, so the capture should give up now.
    pub fn expired(&mut self, token: u64) -> bool {
        if token != self.current || !self.waiting {
            return false;
        }
        self.waiting = false;
        self.current += 1;
        true
    }

    pub fn is_waiting(&self) -> bool {
        self.waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_in_time_is_taken() {
        let mut dog = Watchdog::default();
        let token = dog.begin();
        assert!(dog.is_waiting());
        assert!(dog.answered(token));
        assert!(
            !dog.expired(token),
            "answered, so the limit passing changes nothing"
        );
        assert!(!dog.is_waiting());
    }

    #[test]
    fn a_late_answer_is_ignored() {
        let mut dog = Watchdog::default();
        let token = dog.begin();
        assert!(dog.expired(token), "nothing came: give up");
        assert!(!dog.answered(token), "and an answer after that is ignored");
        // The next capture waits afresh; the old limit can't end it.
        let next = dog.begin();
        assert!(!dog.expired(token));
        assert!(dog.is_waiting());
        assert!(dog.answered(next));
    }
}
