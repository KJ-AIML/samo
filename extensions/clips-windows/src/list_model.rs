//! What the list at Clips' shortcut does, apart from drawing (ADR-005 C2), ported from the Mac's `ListWindow.swift`:
//! type to search; ↑↓ move; Enter or Ctrl+1–9 paste; Alt+Enter pastes plain text; Alt+P pins; Alt+Delete deletes;
//! Esc closes. Pins come first, as the history keeps them. Tested on every OS; `win::list` draws it.

use crate::history::Item;
use crate::search;

/// The keys the list knows; anything else is typed into the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Escape,
    P,
    Delete,
    Backspace,
    /// A number key, 0–9.
    Digit(u8),
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// What a key or a click asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Not the list's key: the search field types it.
    Type,
    /// The list's key, with nothing to do (no row there).
    None,
    /// The selection moved: draw again.
    Moved,
    /// Paste this item where you were; `plain` leaves rich text's formatting out.
    Pick {
        id: String,
        plain: bool,
    },
    /// Pin it, or unpin it if it is pinned.
    Pin(String),
    Delete(String),
    /// Close without pasting.
    Close,
}

/// The search, the rows it shows (by id) and the selected one.
#[derive(Debug, Clone, Default)]
pub struct ListModel {
    query: String,
    shown: Vec<String>,
    selected: usize,
}

impl ListModel {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn shown(&self) -> &[String] {
        &self.shown
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The list opens: no search, the first row selected.
    pub fn open(&mut self, items: &[Item]) {
        self.query.clear();
        self.selected = 0;
        self.reload(items);
    }

    /// The search changed: the first match is selected.
    pub fn search(&mut self, query: &str, items: &[Item]) {
        query.clone_into(&mut self.query);
        self.selected = 0;
        self.reload(items);
    }

    /// Filters again (the history changed); the selection stays where it was, within the rows.
    pub fn reload(&mut self, items: &[Item]) {
        self.shown = search::filter(items, &self.query)
            .into_iter()
            .map(|i| i.id.clone())
            .collect();
        self.selected = self.selected.min(self.shown.len().saturating_sub(1));
    }

    fn at(&self, row: usize) -> Option<String> {
        self.shown.get(row).cloned()
    }

    pub fn key(&mut self, key: Key, mods: Mods) -> Outcome {
        let only_alt = mods.alt && !mods.ctrl;
        let only_ctrl = mods.ctrl && !mods.alt;
        match key {
            Key::Down | Key::Up => {
                if !self.shown.is_empty() {
                    self.selected = if key == Key::Down {
                        (self.selected + 1).min(self.shown.len() - 1)
                    } else {
                        self.selected.saturating_sub(1)
                    };
                }
                Outcome::Moved
            }
            Key::Enter => self.pick(self.selected, mods.alt),
            Key::Escape => Outcome::Close,
            Key::P if only_alt => self.at(self.selected).map_or(Outcome::None, Outcome::Pin),
            Key::Delete | Key::Backspace if only_alt => self
                .at(self.selected)
                .map_or(Outcome::None, Outcome::Delete),
            Key::Digit(n @ 1..=9) if only_ctrl => self.pick(usize::from(n) - 1, false),
            _ => Outcome::Type,
        }
    }

    /// A click on a row; Alt-click pastes plain text.
    pub fn click(&self, row: usize, plain: bool) -> Outcome {
        self.pick(row, plain)
    }

    fn pick(&self, row: usize, plain: bool) -> Outcome {
        self.at(row)
            .map_or(Outcome::None, |id| Outcome::Pick { id, plain })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Capture, History};
    use chrono::Utc;

    fn history(texts: &[&str]) -> History {
        let mut h = History::default();
        for t in texts {
            h.add(&Capture::text(t, Some("notepad.exe")), Utc::now());
        }
        h
    }

    fn titles(m: &ListModel, h: &History) -> Vec<String> {
        m.shown()
            .iter()
            .filter_map(|id| h.item(id).map(|i| i.text.clone()))
            .collect()
    }

    const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
    };
    const ALT: Mods = Mods {
        ctrl: false,
        alt: true,
        shift: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
    };

    #[test]
    fn typing_filters_and_selects_the_first() {
        let h = history(&["apple pie", "banana", "Apple juice"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        assert_eq!(m.shown().len(), 3);
        m.key(Key::Down, NONE);
        m.search("apple", h.items());
        assert_eq!(titles(&m, &h), ["Apple juice", "apple pie"]);
        assert_eq!(m.selected(), 0);
        m.search("ไม่มี", h.items());
        assert!(m.shown().is_empty());
        assert_eq!(m.key(Key::Enter, NONE), Outcome::None, "nothing to pick");
    }

    #[test]
    fn arrows_move_within_the_rows() {
        let h = history(&["one", "two", "three"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        assert_eq!(m.key(Key::Up, NONE), Outcome::Moved);
        assert_eq!(m.selected(), 0);
        m.key(Key::Down, NONE);
        m.key(Key::Down, NONE);
        m.key(Key::Down, NONE);
        assert_eq!(m.selected(), 2);
    }

    #[test]
    fn enter_picks_the_selected_and_alt_enter_plain() {
        let h = history(&["one", "two"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        m.key(Key::Down, NONE);
        let one = h.items().get(1).map(|i| i.id.clone()).unwrap_or_default();
        assert_eq!(
            m.key(Key::Enter, NONE),
            Outcome::Pick {
                id: one.clone(),
                plain: false
            }
        );
        assert_eq!(
            m.key(Key::Enter, ALT),
            Outcome::Pick {
                id: one,
                plain: true
            }
        );
    }

    #[test]
    fn ctrl_and_a_number_picks_that_row() {
        let h = history(&["one", "two", "three"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        let third = h.items().get(2).map(|i| i.id.clone()).unwrap_or_default();
        assert_eq!(
            m.key(Key::Digit(3), CTRL),
            Outcome::Pick {
                id: third,
                plain: false
            }
        );
        assert_eq!(m.key(Key::Digit(9), CTRL), Outcome::None, "no ninth row");
        assert_eq!(
            m.key(Key::Digit(3), NONE),
            Outcome::Type,
            "a plain 3 is typed"
        );
    }

    #[test]
    fn alt_p_pins_and_the_pin_moves_to_the_top() {
        let mut h = history(&["one", "two", "three"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        m.key(Key::Down, NONE);
        m.key(Key::Down, NONE);
        let pinned = m.key(Key::P, ALT);
        let Outcome::Pin(id) = pinned else {
            return assert_eq!(pinned, Outcome::Pin(String::new()), "Alt+P didn't pin");
        };
        h.pin(&id, true);
        m.reload(h.items());
        assert_eq!(m.shown().first(), Some(&id));
        assert_eq!(m.key(Key::P, NONE), Outcome::Type, "a plain p is typed");
    }

    #[test]
    fn alt_delete_deletes_and_the_selection_stays_in_range() {
        let mut h = history(&["one", "two"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        m.key(Key::Down, NONE);
        let deleted = m.key(Key::Delete, ALT);
        let Outcome::Delete(id) = deleted else {
            return assert_eq!(
                deleted,
                Outcome::Delete(String::new()),
                "Alt+Delete didn't delete"
            );
        };
        h.delete(&id);
        m.reload(h.items());
        assert_eq!(m.shown().len(), 1);
        assert_eq!(m.selected(), 0);
        assert_eq!(
            m.key(Key::Delete, NONE),
            Outcome::Type,
            "Delete edits the search"
        );
    }

    #[test]
    fn esc_closes_without_a_pick() {
        let h = history(&["one"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        assert_eq!(m.key(Key::Escape, NONE), Outcome::Close);
    }

    #[test]
    fn opening_again_starts_fresh() {
        let h = history(&["one", "two"]);
        let mut m = ListModel::default();
        m.search("two", h.items());
        m.open(h.items());
        assert_eq!(m.query(), "");
        assert_eq!(m.shown().len(), 2);
        assert_eq!(m.selected(), 0);
    }

    #[test]
    fn a_click_picks_that_row() {
        let h = history(&["one", "two"]);
        let mut m = ListModel::default();
        m.reload(h.items());
        let second = h.items().get(1).map(|i| i.id.clone()).unwrap_or_default();
        assert_eq!(
            m.click(1, false),
            Outcome::Pick {
                id: second,
                plain: false
            }
        );
        assert_eq!(m.click(5, false), Outcome::None);
    }
}
