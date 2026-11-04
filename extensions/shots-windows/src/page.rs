//! Shots' page in Samo (ADR-004 S6), drawn by Samo with its own widgets: the capture buttons, recent shots with their
//! buttons, the shortcuts and whether each works, and what happens after a capture. Windows asks for no permission,
//! so there is no Screen Recording section. Ported from the Mac's `Page.swift`.

use crate::settings::{After, Format, MODES, Recent, RecentShot, Settings, Then};
use chrono::{DateTime, Local, Utc};
use samo_ext_protocol::{ButtonKind, ChoiceOption, Lamp, NoteKind, RowAction, RowItem, Widget};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Everything the page shows.
#[derive(Debug, Clone)]
pub struct PageState {
    pub settings: Settings,
    pub recent: Recent,
    /// Capture modes whose shortcut Windows refused.
    pub refused: BTreeSet<String>,
    /// Capture modes whose shortcut may clash, with the words why.
    pub clashes: BTreeMap<String, String>,
    /// The folder is inside OneDrive: shots saved there are uploaded.
    /// Where shots are saved, as a path (`~` already your profile folder).
    pub folder: PathBuf,
    pub folder_in_onedrive: bool,
    pub now: DateTime<Utc>,
}

/// Midnight today, here.
fn today_start(now: DateTime<Utc>) -> DateTime<Utc> {
    now.with_timezone(&Local)
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|m| m.and_local_timezone(Local).earliest())
        .map_or(now, |m| m.with_timezone(&Utc))
}

/// A file's size in Explorer's units (1,024 bytes a KB): `814 KB`, `2.0 MB`.
pub fn file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} bytes")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < 10.0 * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b < KB * KB * KB {
        format!("{:.0} MB", b / (KB * KB))
    } else {
        format!("{:.1} GB", b / (KB * KB * KB))
    }
}

fn note(text: impl Into<String>, kind: NoteKind) -> Widget {
    Widget::Note {
        text: text.into(),
        kind,
    }
}

fn button(action: &str, label: &str, kind: ButtonKind) -> Widget {
    Widget::Button {
        action: action.into(),
        label: label.into(),
        kind,
    }
}

fn section(title: &str, about: Option<String>) -> Widget {
    Widget::Section {
        title: title.into(),
        about,
    }
}

fn choice(key: &str, label: &str, options: &[(&str, &str)], selected: &str) -> Widget {
    Widget::Choice {
        key: key.into(),
        label: label.into(),
        options: options
            .iter()
            .map(|(id, label)| ChoiceOption {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect(),
        selected: selected.into(),
    }
}

fn recent_row(shot: &RecentShot, now: DateTime<Utc>) -> RowItem {
    let at = shot.at.with_timezone(&Local);
    let time = if at.date_naive() == now.with_timezone(&Local).date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%b %-d, %H:%M").to_string()
    };
    let size = if shot.bytes > 0 {
        format!("{} · ", file_size(shot.bytes))
    } else {
        String::new()
    };
    let action = |id: &str, label: &str| RowAction {
        id: id.into(),
        label: label.into(),
    };
    RowItem {
        id: Some(shot.path.clone()),
        lamp: Lamp::Done,
        title: shot
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&shot.path)
            .to_string(),
        detail: format!("{size}{time}"),
        trailing: String::new(),
        tag: None,
        actions: vec![
            action("copy", "Copy"),
            action("reveal", "Show folder"),
            action("pin", "Pin"),
            action("edit", "Edit"),
        ],
    }
}

pub fn build(s: &PageState) -> Vec<Widget> {
    let mut page = vec![section("CAPTURE", None)];
    for (action, label) in [
        ("area", "Capture an area"),
        ("window", "Capture a window"),
        ("screen", "Capture the screen"),
        ("scroll", "Scrolling capture"),
        ("text", "Copy text from an area"),
        ("colour", "Pick a colour"),
        ("ruler", "Measure"),
    ] {
        page.push(button(action, label, ButtonKind::Default));
    }
    let today = s.recent.count_since(today_start(s.now));
    page.push(section(
        "RECENT",
        (today > 0).then(|| format!("{today} today")),
    ));
    if s.recent.shots().is_empty() {
        page.push(note("Shots you save appear here.", NoteKind::Plain));
    } else {
        page.push(Widget::Rows {
            items: s
                .recent
                .shots()
                .iter()
                .map(|shot| recent_row(shot, s.now))
                .collect(),
        });
    }
    page.push(section("SHORTCUTS", None));
    page.push(Widget::Rows {
        items: MODES
            .iter()
            .map(|(mode, label)| {
                let shortcut = s.settings.shortcuts.get(*mode);
                let keys = shortcut.map_or_else(|| "none".to_string(), ToString::to_string);
                let (lamp, detail) = if s.refused.contains(*mode) {
                    (
                        Lamp::Error,
                        format!("Windows refused {keys}; choose another"),
                    )
                } else if let Some(clash) = s.clashes.get(*mode) {
                    (Lamp::NeedsYou, clash.clone())
                } else if shortcut.is_some() {
                    (Lamp::Done, "Ready".into())
                } else {
                    (Lamp::Off, "No shortcut".into())
                };
                RowItem {
                    id: None,
                    lamp,
                    title: (*label).into(),
                    detail,
                    trailing: keys,
                    tag: None,
                    actions: Vec::new(),
                }
            })
            .collect(),
    });
    page.push(note(
        "In the overlay: Space for a window, S to scroll, T to copy text, C for a colour, R to measure; Esc cancels.",
        NoteKind::Plain,
    ));
    page.push(button(
        "change-shortcuts",
        "Change shortcuts…",
        ButtonKind::Link,
    ));
    page.push(section("AFTER A CAPTURE", None));
    page.push(choice(
        "after",
        "First",
        &[("copy", "Copy"), ("save", "Save"), ("both", "Both")],
        s.settings.after.key(),
    ));
    page.push(choice(
        "then",
        "Then",
        &[("edit", "Edit"), ("pin", "Pin"), ("nothing", "Nothing")],
        s.settings.then.key(),
    ));
    page.push(choice(
        "format",
        "Saved as",
        &[("png", "PNG"), ("jpeg", "JPEG")],
        s.settings.format.key(),
    ));
    page.push(Widget::Switch {
        key: "retina".into(),
        label: "Full resolution".into(),
        on: s.settings.retina,
        note: Some("Off saves fewer pixels on a scaled monitor (125 % and up)".into()),
    });
    page.push(Widget::Switch {
        key: "sound".into(),
        label: "Shutter sound".into(),
        on: s.settings.sound,
        note: None,
    });
    page.push(note(
        format!("Saved to {}", s.folder.display()),
        NoteKind::Plain,
    ));
    if s.folder_in_onedrive {
        page.push(note(
            "That folder is in OneDrive: shots saved there are uploaded.",
            NoteKind::Error,
        ));
    }
    page.push(button("change-folder", "Change folder…", ButtonKind::Link));
    page
}

/// The quick panel's line in Samo: nothing waits for you, so no count.
pub fn badge(recent: &Recent, now: DateTime<Utc>) -> (u32, String) {
    let today = recent.count_since(today_start(now));
    let summary = if today > 0 {
        format!("Shots · {today} today")
    } else {
        "Shots".to_string()
    };
    (0, summary)
}

/// Whether `folder` is inside one of OneDrive's folders (Pictures is often moved there).
pub fn in_onedrive(folder: &Path, roots: &[PathBuf]) -> bool {
    // Windows' paths aren't case-sensitive, and a missed warning would upload shots unannounced.
    let parts = |p: &Path| -> Vec<String> {
        p.components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let folder = parts(folder);
    roots.iter().any(|root| folder.starts_with(&parts(root)))
}

impl Settings {
    /// A switch from Samo's page. False for a key Shots doesn't have.
    pub fn apply_setting(&mut self, key: &str, value: bool) -> bool {
        match key {
            "sound" => self.sound = value,
            "retina" => self.retina = value,
            _ => return false,
        }
        true
    }

    /// A choice from Samo's page. False for a key or value Shots doesn't have.
    pub fn apply_choice(&mut self, key: &str, value: &str) -> bool {
        match key {
            "after" => After::from_key(value).map(|v| self.after = v).is_some(),
            "then" => Then::from_key(value).map(|v| self.then = v).is_some(),
            "format" => Format::from_key(value).map(|v| self.format = v).is_some(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn state(settings: Settings, recent: Recent) -> PageState {
        PageState {
            settings,
            recent,
            refused: BTreeSet::new(),
            clashes: BTreeMap::new(),
            folder: PathBuf::from("/h/Pictures/Shots"),
            folder_in_onedrive: false,
            now: Utc::now(),
        }
    }

    fn texts(page: &[Widget]) -> Vec<String> {
        page.iter()
            .filter_map(|w| match w {
                Widget::Note { text, .. } => Some(text.clone()),
                Widget::Button { action, .. } => Some(format!("button:{action}")),
                Widget::Section { title, .. } => Some(format!("section:{title}")),
                _ => None,
            })
            .collect()
    }

    fn shortcut_rows(page: &[Widget]) -> Vec<RowItem> {
        page.iter()
            .find_map(|w| match w {
                Widget::Rows { items } if items.first().is_some_and(|r| r.id.is_none()) => {
                    Some(items.clone())
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn windows_asks_for_no_permission() {
        let all = texts(&build(&state(Settings::default(), Recent::default())));
        assert!(
            !all.iter()
                .any(|t| t.contains("SCREEN RECORDING") || t.contains("allow")),
            "{all:?}"
        );
        assert_eq!(all.first().map(String::as_str), Some("section:CAPTURE"));
    }

    #[test]
    fn recent_rows_have_actions() -> Result<(), String> {
        let mut recent = Recent::default();
        let now = Utc::now();
        recent.add(RecentShot {
            path: "C:/Users/kj/Pictures/Shots/Shot a.png".into(),
            bytes: 2_100_000,
            at: now,
        });
        let page = build(&state(Settings::default(), recent.clone()));
        let row = page
            .iter()
            .find_map(|w| match w {
                Widget::Rows { items } => items.first().filter(|r| r.id.is_some()).cloned(),
                _ => None,
            })
            .ok_or("no recent row")?;
        assert_eq!(
            row.id.as_deref(),
            Some("C:/Users/kj/Pictures/Shots/Shot a.png")
        );
        assert_eq!(row.title, "Shot a.png");
        // Edit is the fourth, the most a row may carry.
        let actions: Vec<&str> = row.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(actions, ["copy", "reveal", "pin", "edit"]);
        assert_eq!(
            row.actions.get(1).map(|a| a.label.as_str()),
            Some("Show folder")
        );
        assert!(row.detail.starts_with("2.0 MB · "), "{}", row.detail);
        assert!(page.contains(&Widget::Section {
            title: "RECENT".into(),
            about: Some("1 today".into())
        }));
        assert_eq!(badge(&recent, now), (0, "Shots · 1 today".to_string()));
        assert_eq!(badge(&Recent::default(), now), (0, "Shots".to_string()));
        Ok(())
    }

    #[test]
    fn choices_update_settings() {
        let mut s = Settings::default();
        assert!(s.apply_choice("after", "save"));
        assert_eq!(s.after, After::Save);
        assert!(s.apply_choice("format", "jpeg"));
        assert_eq!(s.format, Format::Jpeg);
        assert!(!s.apply_choice("after", "explode"));
        assert!(!s.apply_choice("colour", "red"));
        assert!(!s.apply_choice("after", "pin"), "pin is a Then now");
        assert_eq!(s.after, After::Save, "a bad value changes nothing");
        assert!(s.apply_choice("then", "pin"));
        assert_eq!(s.then, Then::Pin);
        assert!(s.apply_setting("sound", true));
        assert!(!s.apply_setting("other", true));
        let page = build(&state(s, Recent::default()));
        let chosen: Vec<(String, String, Vec<String>)> = page
            .iter()
            .filter_map(|w| match w {
                Widget::Choice {
                    key,
                    selected,
                    options,
                    ..
                } => Some((
                    key.clone(),
                    selected.clone(),
                    options.iter().map(|o| o.id.clone()).collect(),
                )),
                _ => None,
            })
            .collect();
        assert!(chosen.contains(&(
            "after".into(),
            "save".into(),
            vec!["copy".into(), "save".into(), "both".into()]
        )));
        assert!(chosen.contains(&(
            "then".into(),
            "pin".into(),
            vec!["edit".into(), "pin".into(), "nothing".into()]
        )));
    }

    #[test]
    fn a_folder_in_onedrive_is_called_out() {
        let roots = [PathBuf::from("C:/Users/kj/OneDrive")];
        assert!(in_onedrive(
            Path::new("C:/Users/kj/OneDrive/Pictures/Shots"),
            &roots
        ));
        assert!(!in_onedrive(
            Path::new("C:/Users/kj/Pictures/Shots"),
            &roots
        ));
        assert!(
            !in_onedrive(Path::new("C:/Users/kj/OneDrive2/x"), &roots),
            "a sibling isn't inside"
        );
        let mut s = state(Settings::default(), Recent::default());
        s.folder_in_onedrive = true;
        assert!(build(&s).iter().any(|w| matches!(w,
            Widget::Note { text, kind: NoteKind::Error } if text.contains("OneDrive"))));
    }

    #[test]
    fn the_page_has_every_capture_and_shortcut() {
        let mut s = state(Settings::default(), Recent::default());
        s.refused.insert("area".into());
        s.clashes.insert(
            "screen".into(),
            "May clash with ShareX, which is running".into(),
        );
        let page = build(&s);
        let all = texts(&page);
        for action in [
            "area", "window", "screen", "scroll", "text", "colour", "ruler",
        ] {
            assert!(all.contains(&format!("button:{action}")), "{action}");
        }
        // Every mode has a shortcut row; the new ones say they have none.
        let rows = shortcut_rows(&page);
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Whole screen",
                "An area",
                "Scrolling capture",
                "Copy text",
                "Pick a colour",
                "Measure"
            ]
        );
        let lamp = |i: usize| {
            rows.get(i)
                .map(|r| (r.lamp, r.detail.clone(), r.trailing.clone()))
        };
        assert_eq!(
            lamp(0),
            Some((
                Lamp::NeedsYou,
                "May clash with ShareX, which is running".into(),
                "Win+Alt+F".into()
            ))
        );
        assert_eq!(
            lamp(1),
            Some((
                Lamp::Error,
                "Windows refused Win+Alt+S; choose another".into(),
                "Win+Alt+S".into()
            ))
        );
        assert_eq!(
            lamp(2),
            Some((Lamp::Off, "No shortcut".into(), "none".into()))
        );
    }

    #[test]
    fn sizes_read_as_files_do() {
        assert_eq!(file_size(512), "512 bytes");
        // Explorer's units: 1,024 bytes a KB.
        assert_eq!(file_size(834_000), "814 KB");
        assert_eq!(file_size(999_999), "977 KB");
        assert_eq!(file_size(2_100_000), "2.0 MB");
        assert_eq!(file_size(12_000_000), "11 MB");
    }

    #[test]
    fn the_save_folder_reads_as_a_windows_path() {
        let mut s = state(Settings::default(), Recent::default());
        s.folder = PathBuf::from("C:/Users/kj/Pictures/Shots");
        let shown = format!("Saved to {}", s.folder.display());
        assert!(
            build(&s)
                .iter()
                .any(|w| matches!(w, Widget::Note { text, .. } if *text == shown))
        );
    }

    #[test]
    fn onedrive_is_found_whatever_the_case() {
        let roots = [PathBuf::from("C:/Users/kj/OneDrive")];
        assert!(in_onedrive(
            Path::new("c:/users/KJ/onedrive/Pictures"),
            &roots
        ));
    }
}
