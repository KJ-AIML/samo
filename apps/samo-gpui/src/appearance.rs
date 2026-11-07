//! How Samo looks, as you chose it in Settings → Appearance: a **mode** (follow the system, or always light, or
//! always dark) and a **style** (which family of surfaces to draw them in). Kept in `appearance.txt` beside
//! `locations.txt`, two `key = value` lines. A missing, unreadable or unknown value is the default, so a bad file
//! can never stop Samo from drawing. Nothing here goes online.

use std::io;
use std::path::{Path, PathBuf};

/// Light or dark: the system's, or one you fixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::System, Mode::Light, Mode::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Mode::System => "system",
            Mode::Light => "light",
            Mode::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::System => "System",
            Mode::Light => "Light",
            Mode::Dark => "Dark",
        }
    }

    fn from_key(key: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// A family of surfaces. Both are mixed from Talvyn's nine colours; they differ in the tint of the neutrals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Samo's own: Ink-tinted neutrals (a slate with a trace of blue) and white cards.
    #[default]
    Cobalt,
    /// True greys with more depth: a grey canvas under white cards in light, charcoal in dark. No blue in the frame.
    Graphite,
}

impl Style {
    pub const ALL: [Style; 2] = [Style::Cobalt, Style::Graphite];

    pub fn key(self) -> &'static str {
        match self {
            Style::Cobalt => "cobalt",
            Style::Graphite => "graphite",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Style::Cobalt => "Cobalt",
            Style::Graphite => "Graphite",
        }
    }

    /// One line under the choice.
    pub fn about(self) -> &'static str {
        match self {
            Style::Cobalt => "Slate neutrals with a trace of blue. Samo's own.",
            Style::Graphite => "Neutral greys with more depth. No blue in the frame.",
        }
    }

    pub fn from_key(key: &str) -> Option<Style> {
        Style::ALL.into_iter().find(|s| s.key() == key)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Appearance {
    pub mode: Mode,
    pub style: Style,
}

impl Appearance {
    /// Reads the file's text. Order, spacing, case, blank lines, comments and unknown keys or values don't matter;
    /// whatever can't be understood stays the default.
    pub fn parse(text: &str) -> Appearance {
        let mut out = Appearance::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim().to_ascii_lowercase());
            match key.to_ascii_lowercase().as_str() {
                "mode" => out.mode = Mode::from_key(&value).unwrap_or_default(),
                "style" => out.style = Style::from_key(&value).unwrap_or_default(),
                _ => {}
            }
        }
        out
    }

    pub fn to_text(self) -> String {
        format!("mode = {}\nstyle = {}\n", self.mode.key(), self.style.key())
    }
}

/// Where the choice is kept, next to `locations.txt` (the Mac's Application Support, Windows' `%APPDATA%`).
fn store() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA")
            .map(|dir| PathBuf::from(dir).join("Samo").join("appearance.txt"));
    }
    let home = crate::theme::home_dir();
    (!home.is_empty())
        .then(|| PathBuf::from(home).join("Library/Application Support/Samo/appearance.txt"))
}

fn load_from(path: &Path) -> Appearance {
    std::fs::read_to_string(path)
        .map(|text| Appearance::parse(&text))
        .unwrap_or_default()
}

/// Written beside the file and moved into place, so a crash can't leave half a line behind.
fn save_to(path: &Path, appearance: Appearance) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("txt.tmp");
    std::fs::write(&tmp, appearance.to_text())?;
    std::fs::rename(&tmp, path)
}

/// What you chose last time, or the defaults.
pub fn load() -> Appearance {
    store().map(|p| load_from(&p)).unwrap_or_default()
}

/// Keeps `appearance` for next launch.
pub fn save(appearance: Appearance) -> io::Result<()> {
    match store() {
        Some(path) => save_to(&path, appearance),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_system_and_cobalt() {
        assert_eq!(
            Appearance::default(),
            Appearance {
                mode: Mode::System,
                style: Style::Cobalt
            }
        );
        assert_eq!(Appearance::parse(""), Appearance::default());
    }

    #[test]
    fn what_is_written_is_read_back() {
        for mode in Mode::ALL {
            for style in Style::ALL {
                let a = Appearance { mode, style };
                assert_eq!(Appearance::parse(&a.to_text()), a);
            }
        }
    }

    #[test]
    fn spacing_case_and_noise_are_forgiven() {
        let a = Appearance::parse(
            "# my look\n\n  STYLE=Graphite \nmode =  DARK\nfont = big\nnonsense\n",
        );
        assert_eq!(a.mode, Mode::Dark);
        assert_eq!(a.style, Style::Graphite);
    }

    #[test]
    fn an_unknown_value_is_the_default_not_an_error() {
        let a = Appearance::parse("mode = twilight\nstyle = neon\n");
        assert_eq!(a, Appearance::default());
        // One bad line doesn't spoil the good one.
        let b = Appearance::parse("mode = twilight\nstyle = graphite\n");
        assert_eq!(b.mode, Mode::System);
        assert_eq!(b.style, Style::Graphite);
    }

    #[test]
    fn it_survives_a_round_trip_through_a_file() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        // A folder that doesn't exist yet is made.
        let path = tmp.path().join("Samo").join("appearance.txt");
        assert_eq!(load_from(&path), Appearance::default(), "no file: defaults");

        let chosen = Appearance {
            mode: Mode::Light,
            style: Style::Graphite,
        };
        save_to(&path, chosen).map_err(|e| e.to_string())?;
        assert_eq!(load_from(&path), chosen);

        let again = Appearance {
            mode: Mode::Dark,
            style: Style::Cobalt,
        };
        save_to(&path, again).map_err(|e| e.to_string())?;
        assert_eq!(load_from(&path), again, "a second save replaces the first");
        assert!(
            !path.with_extension("txt.tmp").exists(),
            "no temporary file is left behind"
        );
        Ok(())
    }
}
