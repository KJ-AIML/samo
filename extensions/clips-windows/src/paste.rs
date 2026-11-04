//! Pasting where you were (ADR-005 C3, R24). When the list opens, Clips remembers the window in front. A pick puts
//! the item on the clipboard, closes the list, brings that window back, and presses Ctrl+V only once it is in front
//! again. Nothing presses Ctrl+V but a pick. If the window never comes back, or runs as administrator (Windows
//! doesn't let a normal app type into it), it stops at the clipboard and says so. Ported from `PasteFlow.swift`.

use std::time::Duration;

/// What the flow needs from Windows; tests stand in for all of it.
pub trait Windows {
    /// A window: Windows' handle, or a number in tests.
    type Window: PartialEq;
    fn front(&self) -> Option<Self::Window>;
    /// Asks `target` to come forward.
    fn activate(&self, target: &Self::Window);
    /// Whether `target`'s program runs as administrator and Clips doesn't (Windows won't let Clips type into it).
    fn elevated(&self, target: &Self::Window) -> bool;
    /// Presses Ctrl+V.
    fn press_paste(&self);
    fn sleep(&self, time: Duration);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pasted,
    /// On the clipboard only, with what to tell you.
    CopiedOnly(String),
}

pub const COPIED: &str = "Copied: press Ctrl+V";
/// How long to wait for the window to be in front again, and how often to look.
const PATIENCE: Duration = Duration::from_millis(600);
const STEP: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct PasteFlow<W> {
    /// The window in front when the list opened.
    previous: Option<W>,
}

impl<W> Default for PasteFlow<W> {
    fn default() -> Self {
        Self { previous: None }
    }
}

impl<W: PartialEq> PasteFlow<W> {
    /// The list is opening: remember where to paste.
    pub fn remember(&mut self, front: Option<W>) {
        self.previous = front;
    }

    /// The list closed without a pick.
    pub fn cancelled(&mut self) {
        self.previous = None;
    }

    /// A pick: the item is already on the clipboard and the list closed. Paste into the remembered window.
    pub fn picked(&mut self, w: &impl Windows<Window = W>) -> Outcome {
        let Some(target) = self.previous.take() else {
            return Outcome::CopiedOnly(COPIED.into());
        };
        if w.elevated(&target) {
            return Outcome::CopiedOnly(format!(
                "{COPIED} (Windows doesn't let Clips paste into an app run as administrator)"
            ));
        }
        w.activate(&target);
        let mut waited = Duration::ZERO;
        loop {
            if w.front().as_ref() == Some(&target) {
                // A moment for the window to take the keyboard back; then it must still be in front (R24).
                w.sleep(Duration::from_millis(50));
                if w.front().as_ref() != Some(&target) {
                    return Outcome::CopiedOnly(COPIED.into());
                }
                w.press_paste();
                return Outcome::Pasted;
            }
            if waited >= PATIENCE {
                return Outcome::CopiedOnly(COPIED.into());
            }
            w.sleep(STEP);
            waited += STEP;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// Windows for tests: which window is in front at each look, and what was pressed.
    struct FakeWindows {
        fronts: RefCell<Vec<Option<isize>>>,
        elevated: bool,
        activated: Cell<Option<isize>>,
        pressed: Cell<u32>,
    }

    impl FakeWindows {
        fn new(fronts: Vec<Option<isize>>) -> Self {
            Self {
                fronts: RefCell::new(fronts),
                elevated: false,
                activated: Cell::new(None),
                pressed: Cell::new(0),
            }
        }
    }

    impl Windows for FakeWindows {
        type Window = isize;
        fn front(&self) -> Option<isize> {
            let mut fronts = self.fronts.borrow_mut();
            if fronts.len() > 1 {
                fronts.remove(0)
            } else {
                fronts.first().copied().flatten()
            }
        }
        fn activate(&self, target: &isize) {
            self.activated.set(Some(*target));
        }
        fn elevated(&self, _target: &isize) -> bool {
            self.elevated
        }
        fn press_paste(&self) {
            self.pressed.set(self.pressed.get() + 1);
        }
        fn sleep(&self, _for: Duration) {}
    }

    #[test]
    fn paste_only_follows_a_pick() {
        let w = FakeWindows::new(vec![Some(7)]);
        let mut flow = PasteFlow::default();
        // Nothing remembered: the list wasn't opened over an app.
        assert_eq!(flow.picked(&w), Outcome::CopiedOnly(COPIED.into()));
        assert_eq!(w.pressed.get(), 0);
        // Remembered, then closed without a pick: nothing is pasted later.
        flow.remember(Some(7));
        flow.cancelled();
        assert_eq!(flow.picked(&w), Outcome::CopiedOnly(COPIED.into()));
        assert_eq!(w.pressed.get(), 0);
    }

    #[test]
    fn paste_waits_for_the_right_window() {
        // It comes back a moment late: pasted once it is in front.
        let w = FakeWindows::new(vec![Some(1), Some(1), Some(7)]);
        let mut flow = PasteFlow::default();
        flow.remember(Some(7));
        assert_eq!(flow.picked(&w), Outcome::Pasted);
        assert_eq!((w.activated.get(), w.pressed.get()), (Some(7), 1));
        // A second pick without opening the list again pastes nothing.
        assert_eq!(flow.picked(&w), Outcome::CopiedOnly(COPIED.into()));
        assert_eq!(w.pressed.get(), 1);

        // It never comes back.
        let w = FakeWindows::new(vec![Some(1)]);
        flow.remember(Some(7));
        assert_eq!(flow.picked(&w), Outcome::CopiedOnly(COPIED.into()));
        assert_eq!(w.pressed.get(), 0);

        // It comes forward, then something else takes the front before Ctrl+V: nothing is pressed.
        let w = FakeWindows::new(vec![Some(7), Some(3)]);
        flow.remember(Some(7));
        assert_eq!(flow.picked(&w), Outcome::CopiedOnly(COPIED.into()));
        assert_eq!(w.pressed.get(), 0, "Ctrl+V went to another window");

        // Run as administrator: Windows doesn't let Clips type into it.
        let mut w = FakeWindows::new(vec![Some(7)]);
        w.elevated = true;
        flow.remember(Some(7));
        assert!(
            matches!(flow.picked(&w), Outcome::CopiedOnly(why) if why.contains("administrator"))
        );
        assert_eq!(w.pressed.get(), 0);
    }
}
