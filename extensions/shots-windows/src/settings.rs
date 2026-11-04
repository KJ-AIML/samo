//! Shots' settings and its recent list, kept in its `data/` folder (`settings.json`, `recent.json`). Only file paths
//! are kept, never images: shots live where you saved them. Ported from the Mac's `Settings.swift`; its shortcuts are
//! Windows' (Win, Ctrl, Alt, Shift and a key), and `~` is your profile folder.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR, Path, PathBuf};

/// First, the shot is copied, saved, or both (R19: before any window opens).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    Copy,
    Save,
    Both,
}

/// Then it opens in the editor, is pinned, or nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Edit,
    Pin,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

impl After {
    pub fn key(self) -> &'static str {
        match self {
            After::Copy => "copy",
            After::Save => "save",
            After::Both => "both",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        [After::Copy, After::Save, After::Both]
            .into_iter()
            .find(|a| a.key() == key)
    }
}

impl Then {
    pub fn key(self) -> &'static str {
        match self {
            Then::Edit => "edit",
            Then::Pin => "pin",
            Then::Nothing => "nothing",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        [Then::Edit, Then::Pin, Then::Nothing]
            .into_iter()
            .find(|t| t.key() == key)
    }
}

impl Format {
    pub fn key(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpeg",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        [Format::Png, Format::Jpeg]
            .into_iter()
            .find(|f| f.key() == key)
    }
}

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
    /// Win+Alt+F, the whole screen. (Win+Shift+S is Windows' own Snipping Tool; Win with Alt or Shift and a number is
    /// the taskbar's.)
    pub fn whole_screen() -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: false,
            win: true,
            key: 'F',
        }
    }

    /// Win+Alt+S, an area.
    pub fn area() -> Self {
        Self {
            key: 'S',
            ..Self::whole_screen()
        }
    }

    /// Whether it can be held for all of Windows: without Ctrl, Alt or Win it would take a key you type.
    pub fn can_be_global(&self) -> bool {
        self.ctrl || self.alt || self.win
    }

    fn to_value(self) -> Value {
        json!({ "ctrl": self.ctrl, "alt": self.alt, "shift": self.shift, "win": self.win, "key": self.key.to_string() })
    }

    fn from_value(v: &Value) -> Option<Self> {
        let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        // A letter or digit, as Windows holds it; anything else is no shortcut.
        let key = v
            .get("key")?
            .as_str()?
            .chars()
            .next()
            .filter(char::is_ascii_alphanumeric)?
            .to_ascii_uppercase();
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

/// Everything a shortcut can start, in order: the whole screen and an area have one; the rest wait to be chosen, so
/// Shots takes nothing more from Windows (R18).
pub const MODES: [(&str, &str); 6] = [
    ("screen", "Whole screen"),
    ("area", "An area"),
    ("scroll", "Scrolling capture"),
    ("text", "Copy text"),
    ("colour", "Pick a colour"),
    ("ruler", "Measure"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub after: After,
    pub then: Then,
    /// Where shots are saved; `~` is your profile folder.
    pub folder: String,
    pub format: Format,
    /// Full resolution, or half the pixels on a high-DPI screen.
    pub retina: bool,
    pub sound: bool,
    /// By capture mode; a mode without one has no shortcut.
    pub shortcuts: BTreeMap<String, Shortcut>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            after: After::Both,
            then: Then::Edit,
            folder: "~/Pictures/Shots".into(),
            format: Format::Png,
            retina: true,
            sound: false,
            shortcuts: BTreeMap::from([
                ("screen".to_string(), Shortcut::whole_screen()),
                ("area".to_string(), Shortcut::area()),
            ]),
        }
    }
}

/// Writes `bytes` to `to` through a temporary file beside it, so a crash never leaves half a file.
fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("couldn't make Shots' folder: {e}"))?;
    let to = dir.join(name);
    let tmp = dir.join(format!("{name}.tmp-{}", std::process::id()));
    fs::write(&tmp, bytes).map_err(|e| format!("couldn't save {name}: {e}"))?;
    fs::rename(&tmp, &to).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("couldn't save {name}: {e}")
    })
}

impl Settings {
    /// Reads `settings.json`'s text field by field, so one value Shots doesn't know never resets the rest; anything
    /// unreadable is the default. Shots 0.1.0's `"after": "pin"` (pin, without copying or saving) is both, then pin.
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        // A BOM (Windows PowerShell 5.1, older Notepad) is not a damaged file.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
            return s;
        };
        let text_of = |key: &str| map.get(key).and_then(Value::as_str);
        if text_of("after") == Some("pin") {
            s.after = After::Both;
            s.then = Then::Pin;
        } else {
            s.after = text_of("after")
                .and_then(After::from_key)
                .unwrap_or(s.after);
            s.then = text_of("then").and_then(Then::from_key).unwrap_or(s.then);
        }
        if let Some(folder) = text_of("folder") {
            s.folder = folder.to_string();
        }
        s.format = text_of("format")
            .and_then(Format::from_key)
            .unwrap_or(s.format);
        let flag = |key: &str| map.get(key).and_then(Value::as_bool);
        s.retina = flag("retina").unwrap_or(s.retina);
        s.sound = flag("sound").unwrap_or(s.sound);
        if let Some(Value::Object(chosen)) = map.get("shortcuts") {
            s.shortcuts = chosen
                .iter()
                .filter_map(|(mode, v)| Some((mode.clone(), Shortcut::from_value(v)?)))
                .collect();
        }
        s
    }

    pub fn to_json(&self) -> String {
        let shortcuts: Map<String, Value> = self
            .shortcuts
            .iter()
            .map(|(mode, k)| (mode.clone(), k.to_value()))
            .collect();
        json!({
            "after": self.after.key(),
            "then": self.then.key(),
            "folder": self.folder,
            "format": self.format.key(),
            "retina": self.retina,
            "sound": self.sound,
            "shortcuts": shortcuts,
        })
        .to_string()
    }

    pub fn load(data_dir: &Path) -> Self {
        fs::read_to_string(data_dir.join("settings.json"))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        write_atomic(data_dir, "settings.json", self.to_json().as_bytes())
    }

    /// The folder shots are saved to, with `~` as `home`.
    pub fn folder_path(&self, home: &Path) -> PathBuf {
        expand(&self.folder, home)
    }
}

/// Your home folder: `USERPROFILE` on Windows (where Pictures is), `HOME` elsewhere, read through `get`.
pub fn home_from(get: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let name = if windows { "USERPROFILE" } else { "HOME" };
    get(name).filter(|h| !h.is_empty()).map(PathBuf::from)
}

pub fn home() -> Option<PathBuf> {
    home_from(|name| std::env::var_os(name), cfg!(windows))
}

/// `~` and `~/…` (or `~\…`) as paths in `home`.
pub fn expand(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        // One path shape: each part joined with this system's separator.
        Some(rest) => rest
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .fold(home.to_path_buf(), |path, part| path.join(part)),
        None => PathBuf::from(path),
    }
}

/// A path in `home` as `~` and the rest, the way the folder is shown.
pub fn abbreviate(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => {
            let parts: Vec<String> = rest
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            format!("~{MAIN_SEPARATOR}{}", parts.join(MAIN_SEPARATOR_STR))
        }
        Err(_) => path.display().to_string(),
    }
}

/// A saved shot in the recent list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentShot {
    pub path: String,
    pub bytes: u64,
    pub at: DateTime<Utc>,
}

/// The last 20 shots saved, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    shots: Vec<RecentShot>,
}

impl Recent {
    pub const KEEP: usize = 20;

    pub fn shots(&self) -> &[RecentShot] {
        &self.shots
    }

    pub fn add(&mut self, shot: RecentShot) {
        self.shots.retain(|s| s.path != shot.path);
        self.shots.insert(0, shot);
        self.shots.truncate(Self::KEEP);
    }

    /// Drops shots whose files are gone (deleted, or moved away).
    pub fn forget_missing(&mut self, exists: impl Fn(&str) -> bool) {
        self.shots.retain(|s| exists(&s.path));
    }

    /// How many were taken since `start` (today's count, for Samo's quick panel).
    pub fn count_since(&self, start: DateTime<Utc>) -> usize {
        self.shots.iter().filter(|s| s.at >= start).count()
    }

    pub fn load(data_dir: &Path) -> Self {
        fs::read(data_dir.join("recent.json"))
            .ok()
            .and_then(|bytes| {
                let json = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
                serde_json::from_slice(json).ok()
            })
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        write_atomic(data_dir, "recent.json", &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn settings_round_trip() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        // Nothing saved yet: the defaults.
        let fresh = Settings::load(dir.path());
        assert_eq!(fresh, Settings::default());
        assert_eq!(fresh.after, After::Both);
        assert_eq!(
            fresh
                .shortcuts
                .get("area")
                .map(ToString::to_string)
                .as_deref(),
            Some("Win+Alt+S")
        );
        assert_eq!(
            fresh
                .shortcuts
                .get("screen")
                .map(ToString::to_string)
                .as_deref(),
            Some("Win+Alt+F")
        );
        assert!(
            fresh
                .folder_path(Path::new("/h"))
                .ends_with("Pictures/Shots")
        );

        let mut changed = fresh.clone();
        changed.after = After::Save;
        changed.then = Then::Pin;
        changed.format = Format::Jpeg;
        changed.sound = true;
        changed
            .shortcuts
            .insert("area".into(), shortcut(true, false, true, true, '2'));
        changed.save(dir.path())?;
        assert_eq!(Settings::load(dir.path()), changed);
        assert_eq!(
            changed
                .shortcuts
                .get("area")
                .map(ToString::to_string)
                .as_deref(),
            Some("Win+Ctrl+Shift+2")
        );

        // A damaged file reads as the defaults, never as a crash.
        std::fs::write(dir.path().join("settings.json"), "{not json").map_err(|e| e.to_string())?;
        assert_eq!(Settings::load(dir.path()), Settings::default());

        // A shortcut without Ctrl, Alt or Win would take a key you type.
        assert!(!shortcut(false, false, true, false, '2').can_be_global());
        assert!(Shortcut::area().can_be_global());
        Ok(())
    }

    #[test]
    fn phase_one_settings_still_load() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        // Shots 0.1.0's settings, with its "pin": pin without copying or saving.
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"after":"pin","folder":"~/Desktop","format":"jpeg","retina":false,"sound":true,"shortcuts":{}}"#,
        )
        .map_err(|e| e.to_string())?;
        let loaded = Settings::load(dir.path());
        assert_eq!((loaded.after, loaded.then), (After::Both, Then::Pin));
        assert_eq!(loaded.folder, "~/Desktop");
        assert_eq!(loaded.format, Format::Jpeg);
        assert!(!loaded.retina && loaded.sound);
        assert!(loaded.shortcuts.is_empty());
        // The others keep their meaning, opening the editor after.
        std::fs::write(dir.path().join("settings.json"), r#"{"after":"copy"}"#)
            .map_err(|e| e.to_string())?;
        let loaded = Settings::load(dir.path());
        assert_eq!((loaded.after, loaded.then), (After::Copy, Then::Edit));
        Ok(())
    }

    #[test]
    fn one_bad_value_keeps_the_rest() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"after":"teleport","then":"dance","format":"webp","folder":"D:/Shots","sound":true}"#,
        )
        .map_err(|e| e.to_string())?;
        let loaded = Settings::load(dir.path());
        assert_eq!(
            (loaded.after, loaded.then, loaded.format),
            (After::Both, Then::Edit, Format::Png)
        );
        assert_eq!(loaded.folder, "D:/Shots", "what it knows is kept");
        assert!(loaded.sound);
        assert_eq!(loaded.shortcuts.get("area"), Some(&Shortcut::area()));
        // What it saves, it loads.
        let mut changed = loaded;
        changed.then = Then::Nothing;
        changed.save(dir.path())?;
        assert_eq!(Settings::load(dir.path()), changed);
        Ok(())
    }

    #[test]
    fn new_modes_have_no_shortcut_by_default() {
        let fresh = Settings::default();
        let keys: Vec<&str> = fresh.shortcuts.keys().map(String::as_str).collect();
        assert_eq!(keys, ["area", "screen"]);
        for mode in ["scroll", "text", "colour", "ruler"] {
            assert!(!fresh.shortcuts.contains_key(mode), "{mode}");
            assert!(MODES.iter().any(|(key, _)| *key == mode), "{mode}");
        }
        assert_eq!(
            shortcut(true, false, true, false, 't').to_string(),
            "Ctrl+Shift+T"
        );
    }

    #[test]
    fn a_shortcut_chosen_is_kept() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut chosen = Settings::default();
        chosen
            .shortcuts
            .insert("text".into(), shortcut(true, false, true, false, 'T'));
        chosen.save(dir.path())?;
        let loaded = Settings::load(dir.path());
        assert_eq!(
            loaded
                .shortcuts
                .get("text")
                .map(ToString::to_string)
                .as_deref(),
            Some("Ctrl+Shift+T")
        );
        assert_eq!(loaded.shortcuts.get("area"), Some(&Shortcut::area()));
        Ok(())
    }

    #[test]
    fn tilde_is_your_home_folder() {
        let home = Path::new("/tmp/h");
        assert_eq!(
            expand("~/Pictures/Shots", home),
            Path::new("/tmp/h/Pictures/Shots")
        );
        assert_eq!(expand("~", home), home);
        assert_eq!(expand("D:/Shots", home), Path::new("D:/Shots"));
        assert_eq!(
            abbreviate(Path::new("/tmp/h/Pictures"), home),
            format!("~{MAIN_SEPARATOR}Pictures")
        );
        // A sibling that merely starts the same isn't inside the home.
        assert_eq!(
            abbreviate(Path::new("/tmp/h2/Pictures"), home),
            Path::new("/tmp/h2/Pictures").display().to_string()
        );
        // The home is the profile folder on Windows, HOME elsewhere.
        let env = |name: &str| match name {
            "USERPROFILE" => Some("C:/Users/kj".into()),
            "HOME" => Some("/home/kj".into()),
            _ => None,
        };
        assert_eq!(home_from(env, true), Some(PathBuf::from("C:/Users/kj")));
        assert_eq!(home_from(env, false), Some(PathBuf::from("/home/kj")));
        assert_eq!(home_from(|_| None, true), None);
    }

    /// Windows PowerShell 5.1 and older Notepad write a BOM: settings saved that way still load.
    #[test]
    fn a_bom_is_not_a_damaged_file() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        std::fs::write(dir.path().join("settings.json"), "\u{feff}{\"sound\":true}")
            .map_err(|e| e.to_string())?;
        assert!(Settings::load(dir.path()).sound);
        Ok(())
    }

    #[test]
    fn shortcut_keys_are_letters_and_digits() {
        let s = Settings::parse(
            r#"{"shortcuts":{"area":{"win":true,"alt":true,"key":"s"},"text":{"ctrl":true,"key":"ก"}}}"#,
        );
        assert_eq!(
            s.shortcuts.get("area"),
            Some(&Shortcut::area()),
            "lower case is the same key"
        );
        assert!(
            !s.shortcuts.contains_key("text"),
            "a key Windows can't hold is dropped"
        );
    }

    #[test]
    fn a_folder_is_one_path_shape() {
        let home = Path::new("/h");
        let want = home.join("Pictures").join("Shots").display().to_string();
        assert_eq!(expand("~/Pictures/Shots", home).display().to_string(), want);
        assert_eq!(
            expand("~\\Pictures\\Shots", home).display().to_string(),
            want
        );
    }

    #[test]
    fn recent_keeps_twenty() -> Result<(), String> {
        let mut recent = Recent::default();
        let start = DateTime::<Utc>::from_timestamp(1_000_000, 0).ok_or("a time")?;
        for i in 0..25i64 {
            recent.add(RecentShot {
                path: format!("/shots/{i}.png"),
                bytes: u64::try_from(i).unwrap_or(0),
                at: start + chrono::Duration::seconds(i),
            });
        }
        assert_eq!(recent.shots().len(), Recent::KEEP);
        assert_eq!(
            recent.shots().first().map(|s| s.path.as_str()),
            Some("/shots/24.png"),
            "newest first"
        );
        assert_eq!(
            recent.shots().last().map(|s| s.path.as_str()),
            Some("/shots/5.png")
        );
        // Adding a path again moves it to the front, once.
        recent.add(RecentShot {
            path: "/shots/10.png".into(),
            bytes: 10,
            at: start + chrono::Duration::seconds(100),
        });
        assert_eq!(
            recent.shots().first().map(|s| s.path.as_str()),
            Some("/shots/10.png")
        );
        assert_eq!(
            recent
                .shots()
                .iter()
                .filter(|s| s.path == "/shots/10.png")
                .count(),
            1
        );
        assert_eq!(
            recent.count_since(start + chrono::Duration::seconds(20)),
            6,
            "20-24, and 10 again"
        );
        // Files that are gone are forgotten.
        recent.forget_missing(|p| p != "/shots/24.png");
        assert!(!recent.shots().iter().any(|s| s.path == "/shots/24.png"));
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        recent.save(dir.path())?;
        assert_eq!(Recent::load(dir.path()), recent);
        Ok(())
    }
}
