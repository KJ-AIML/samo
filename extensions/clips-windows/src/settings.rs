//! Clips' own settings (`data/settings.json`): its shortcut, or none, and whether it keeps what you copy. A missing
//! shortcut is the default; one saved as none stays none (ADR-005, ported from `ClipsShortcut.swift`).

use serde_json::{Map, Value, json};
use std::fmt;

/// A global shortcut: modifiers and one letter or digit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: char,
}

impl Shortcut {
    /// Win+Alt+V: beside Windows' own clipboard (Win+V); Ctrl+Shift+C opens browsers' inspector, and Ctrl+Alt+… is
    /// AltGr on many keyboards.
    pub fn standard() -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: false,
            win: true,
            key: 'V',
        }
    }

    fn to_value(self) -> Value {
        json!({ "ctrl": self.ctrl, "alt": self.alt, "shift": self.shift, "win": self.win, "key": self.key.to_string() })
    }

    fn from_value(v: &Value) -> Option<Self> {
        let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        let key = v.get("key")?.as_str()?.chars().next()?;
        Some(Self {
            ctrl: flag("ctrl"),
            alt: flag("alt"),
            shift: flag("shift"),
            win: flag("win"),
            key,
        })
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.win, "Win"),
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key.to_ascii_uppercase())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub shortcut: Option<Shortcut>,
    pub watching: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: Some(Shortcut::standard()),
            watching: true,
        }
    }
}

impl Settings {
    /// Reads `settings.json`'s text; anything missing or unreadable is the default.
    pub fn parse(text: &str) -> Self {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let shortcut = match map.get("shortcut") {
            None => Some(Shortcut::standard()),
            Some(Value::Null) => None,
            Some(v) => Shortcut::from_value(v).or(Some(Shortcut::standard())),
        };
        let watching = map.get("watching").and_then(Value::as_bool).unwrap_or(true);
        Self { shortcut, watching }
    }

    pub fn to_json(&self) -> String {
        let mut map = Map::new();
        map.insert(
            "shortcut".into(),
            self.shortcut.map_or(Value::Null, Shortcut::to_value),
        );
        map.insert("watching".into(), Value::Bool(self.watching));
        Value::Object(map).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortcut_defaults_and_can_be_none() {
        // Nothing saved: the default, watching.
        let s = Settings::parse("");
        assert_eq!(s.shortcut, Some(Shortcut::standard()));
        assert!(s.watching);
        assert_eq!(Shortcut::standard().to_string(), "Win+Alt+V");
        // Saved as none, it stays none; round trips keep it.
        let none = Settings {
            shortcut: None,
            watching: false,
        };
        assert_eq!(Settings::parse(&none.to_json()), none);
        let mine = Settings {
            shortcut: Some(Shortcut {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                key: 'K',
            }),
            watching: true,
        };
        assert_eq!(Settings::parse(&mine.to_json()), mine);
        assert_eq!(
            mine.shortcut.map(|s| s.to_string()).as_deref(),
            Some("Ctrl+Shift+K")
        );
        // A damaged file is the defaults.
        assert_eq!(Settings::parse("{not json"), Settings::default());
    }
}
