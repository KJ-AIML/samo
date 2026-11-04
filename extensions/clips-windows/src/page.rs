//! Clips' page in Samo (ADR-005 C4), ported from the Mac's `Page.swift`: pins, the latest copies, its shortcut, and
//! what it keeps. Windows asks for no permission, so there is no permissions section; the shortcut is one of a few,
//! chosen on the page (Windows has no recorder window here).

use crate::history::{History, Item, Kind};
use crate::settings::{Settings, Shortcut};
use chrono::{DateTime, Local};
use samo_ext_protocol::{ButtonKind, ChoiceOption, Lamp, NoteKind, RowAction, RowItem, Widget};
use unicode_normalization::char::is_combining_mark;

/// The latest copies the page lists (the list at the shortcut shows them all).
const RECENT_ON_PAGE: usize = 20;
/// A title longer than this many characters (as people see them) is cut to one fewer and an ellipsis, as on the Mac.
const TITLE_CHARS: usize = 80;

/// Whether the shortcut works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutState {
    /// Nothing holds it yet: the list hasn't said whether Windows gave it (R28).
    NotYet,
    Ready,
    /// Windows refused it: another app has it (R18, never forced).
    Refused,
    /// The list couldn't start, so nothing can hold the shortcut (R28).
    Broken,
}

/// Everything the page shows.
#[derive(Debug, Clone)]
pub struct PageState<'a> {
    pub history: &'a History,
    pub settings: Settings,
    pub shortcut: ShortcutState,
    pub paused_until: Option<DateTime<Local>>,
    pub ignore_next: bool,
    pub confirming_clear: bool,
    pub now: DateTime<Local>,
}

/// The shortcuts the page offers, by choice id; "none" means no shortcut.
pub fn shortcut_choices() -> [(&'static str, Option<Shortcut>); 4] {
    let with = |alt, shift, win| {
        Some(Shortcut {
            ctrl: false,
            alt,
            shift,
            win,
            key: 'V',
        })
    };
    [
        ("win-alt-v", Some(Shortcut::standard())),
        ("win-shift-v", with(false, true, true)),
        ("alt-shift-v", with(true, true, false)),
        ("none", None),
    ]
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Text => "Text",
        Kind::Rich => "Rich text",
        Kind::Image => "Image",
        Kind::Files => "Files",
    }
}

/// A row's time: `14:30` today, else `Nov 4, 09:15`.
fn when(item: &Item, now: DateTime<Local>) -> String {
    let at = item.last.with_timezone(&Local);
    if at.date_naive() == now.date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%b %-d, %H:%M").to_string()
    }
}

/// Whether `c` belongs to the character before it, as people see characters: a mark (Thai vowels and tones above
/// and below, accents), Thai and Lao sara am, a joiner, a variation selector, a skin tone, or an emoji tag.
fn joins(c: char) -> bool {
    is_combining_mark(c)
        || matches!(
            c,
            '\u{0E33}'
                | '\u{0EB3}'
                | '\u{200D}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{1F3FB}'..='\u{1F3FF}'
                | '\u{E0020}'..='\u{E007F}'
        )
}

/// The characters of `text` as people see them: a base with what joins it, an emoji joined by U+200D, a flag's two
/// letters.
fn characters(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut previous: Option<char> = None;
    let mut flag_half = false;
    for (at, c) in text.char_indices() {
        let flag = ('\u{1F1E6}'..='\u{1F1FF}').contains(&c);
        let attached = at > 0 && (joins(c) || previous == Some('\u{200D}') || (flag && flag_half));
        if !attached && at > 0 {
            out.extend(text.get(start..at));
            start = at;
        }
        flag_half = flag && !(attached && flag_half);
        previous = Some(c);
    }
    out.extend(text.get(start..).filter(|rest| !rest.is_empty()));
    out
}

/// `text` if it has at most `max` characters as people see them; else its first `max - 1` and an ellipsis.
pub fn cut(text: &str, max: usize) -> String {
    let chars = characters(text);
    if chars.len() <= max {
        return text.to_string();
    }
    let mut cut: String = chars.iter().take(max.saturating_sub(1)).copied().collect();
    cut.push('…');
    cut
}

fn row(item: &Item, now: DateTime<Local>) -> RowItem {
    let mut title = cut(&item.title(), TITLE_CHARS);
    if title.is_empty() {
        title = " ".into();
    }
    let mut detail = vec![kind_name(item.kind).to_string()];
    if item.copies > 1 {
        detail.push(format!("{} copies", item.copies));
    }
    if let Some(app) = &item.source {
        let name = app
            .strip_suffix(".exe")
            .or_else(|| app.strip_suffix(".EXE"));
        detail.push(name.unwrap_or(app).to_string());
    }
    detail.push(when(item, now));
    let action = |id: &str, label: &str| RowAction {
        id: id.into(),
        label: label.into(),
    };
    RowItem {
        id: Some(item.id.clone()),
        lamp: if item.pinned { Lamp::Done } else { Lamp::Off },
        title,
        detail: detail.join(" · "),
        trailing: String::new(),
        tag: None,
        actions: vec![
            action("copy", "Copy"),
            if item.pinned {
                action("unpin", "Unpin")
            } else {
                action("pin", "Pin")
            },
            action("delete", "Delete"),
        ],
    }
}

fn note(text: impl Into<String>) -> Widget {
    Widget::Note {
        text: text.into(),
        kind: NoteKind::Plain,
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

/// The page.
pub fn build(s: &PageState) -> Vec<Widget> {
    let mut view = Vec::new();
    let shortcut_words = s
        .settings
        .shortcut
        .map_or_else(|| "none".to_string(), |k| k.to_string());

    let pinned = s.history.pinned();
    if !pinned.is_empty() {
        view.push(section("PINNED", Some(pinned.len().to_string())));
        view.push(Widget::Rows {
            items: pinned.iter().map(|i| row(i, s.now)).collect(),
        });
    }
    let recent = s.history.recent();
    view.push(section(
        "RECENT",
        (!recent.is_empty()).then(|| format!("{} kept", recent.len())),
    ));
    if recent.is_empty() {
        view.push(note(match s.settings.shortcut {
            Some(k) => format!("Nothing copied yet. Copy something, then press {k}."),
            None => "Nothing copied yet. Copy something, then choose a shortcut below to find it."
                .into(),
        }));
    } else {
        view.push(Widget::Rows {
            items: recent
                .iter()
                .take(RECENT_ON_PAGE)
                .map(|i| row(i, s.now))
                .collect(),
        });
    }

    view.push(section("SHORTCUT", None));
    let (lamp, detail) = match (s.settings.shortcut, s.shortcut) {
        (None, _) => (Lamp::Off, "No shortcut: the list can't open".to_string()),
        (Some(k), ShortcutState::Refused) => (
            Lamp::Error,
            format!("Windows refused {k}: another app has it. Choose another below"),
        ),
        (Some(_), ShortcutState::Ready) => (Lamp::Done, "Ready".to_string()),
        (Some(_), ShortcutState::NotYet) => {
            (Lamp::Off, "Not ready yet: the list is starting".to_string())
        }
        (Some(_), ShortcutState::Broken) => (
            Lamp::Error,
            "The list couldn't start: turn Clips off and on again".to_string(),
        ),
    };
    view.push(Widget::Rows {
        items: vec![RowItem {
            id: None,
            lamp,
            title: "Open the list".into(),
            detail,
            trailing: shortcut_words,
            tag: None,
            actions: Vec::new(),
        }],
    });
    let selected = shortcut_choices()
        .into_iter()
        .find(|(_, k)| *k == s.settings.shortcut)
        .map_or("win-alt-v", |(id, _)| id);
    view.push(Widget::Choice {
        key: "shortcut".into(),
        label: "Shortcut".into(),
        options: shortcut_choices()
            .into_iter()
            .map(|(id, k)| ChoiceOption {
                id: id.into(),
                label: k.map_or_else(|| "None".to_string(), |k| k.to_string()),
            })
            .collect(),
        selected: selected.into(),
    });
    view.push(note(
        "In the list: type to find, ↑↓ to choose, Enter pastes, Alt+Enter pastes plain text, Ctrl+1–9 picks, \
         Alt+P pins, Alt+Delete deletes, Esc closes.",
    ));

    view.push(section("WHAT CLIPS KEEPS", None));
    view.push(Widget::Switch {
        key: "watching".into(),
        label: "Keep what I copy".into(),
        on: s.settings.watching,
        note: (!s.settings.watching).then(|| "Off: Clips keeps nothing you copy.".to_string()),
    });
    view.push(Widget::Choice {
        key: "limit".into(),
        label: "Keep".into(),
        options: ["50", "200", "500"]
            .into_iter()
            .map(|n| ChoiceOption {
                id: n.into(),
                label: format!("{n} items"),
            })
            .collect(),
        selected: s.history.limit.to_string(),
    });
    if s.settings.watching {
        match s.paused_until.filter(|until| *until > s.now) {
            Some(until) => {
                view.push(note(format!("Paused until {}.", until.format("%H:%M"))));
                view.push(button("resume", "Resume", ButtonKind::Default));
            }
            None => view.push(button("pause", "Pause for 15 minutes", ButtonKind::Default)),
        }
    }
    if s.ignore_next {
        view.push(note("The next copy won't be kept."));
    } else {
        view.push(button(
            "ignore-next",
            "Don't keep the next copy",
            ButtonKind::Default,
        ));
    }
    if s.confirming_clear {
        view.push(note(
            "Clear every copy but the pinned ones? This can't be undone.",
        ));
        view.push(button("clear-confirm", "Clear", ButtonKind::Default));
        view.push(button("clear-cancel", "Keep them", ButtonKind::Link));
    } else if !recent.is_empty() {
        view.push(button("clear", "Clear history…", ButtonKind::Link));
    }
    view.push(note(
        "Never kept: what password managers mark as secret, and anything copied in 1Password, Bitwarden or KeePass. \
         Kept only on this PC; uninstalling deletes it.",
    ));
    view
}

/// The badge: how many copies are kept.
pub fn badge(history: &History) -> (u32, String) {
    let n = history.items().len();
    let summary = match n {
        0 => "Clips".to_string(),
        1 => "Clips · 1 item".to_string(),
        n => format!("Clips · {n} items"),
    };
    (0, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Capture, History};
    use chrono::{Duration, TimeZone};

    fn now() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, 5, 14, 30, 0)
            .single()
            .unwrap_or_default()
    }

    fn state(history: &History) -> PageState<'_> {
        PageState {
            history,
            settings: Settings::default(),
            shortcut: ShortcutState::Ready,
            paused_until: None,
            ignore_next: false,
            confirming_clear: false,
            now: now(),
        }
    }

    fn sections(view: &[Widget]) -> Vec<String> {
        view.iter()
            .filter_map(|w| match w {
                Widget::Section { title, .. } => Some(title.clone()),
                _ => None,
            })
            .collect()
    }

    fn rows(view: &[Widget]) -> Vec<RowItem> {
        view.iter()
            .filter_map(|w| match w {
                Widget::Rows { items } => Some(items.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn has_button(view: &[Widget], action: &str) -> bool {
        view.iter()
            .any(|w| matches!(w, Widget::Button { action: a, .. } if a == action))
    }

    #[test]
    fn the_page_has_no_permissions_on_windows() {
        let h = History::default();
        let view = build(&state(&h));
        assert_eq!(sections(&view), ["RECENT", "SHORTCUT", "WHAT CLIPS KEEPS"]);
        // Nothing copied yet: it says how to start.
        assert!(view.iter().any(
            |w| matches!(w, Widget::Note { text, .. } if text.contains("Nothing copied yet") && text.contains("Win+Alt+V"))
        ));
    }

    #[test]
    fn rows_have_copy_pin_and_delete() {
        let mut h = History::default();
        let t = now().with_timezone(&chrono::Utc);
        h.add(&Capture::text("first", Some("notepad.exe")), t);
        let pinned = h.add(&Capture::text("kept", None), t).item.id.clone();
        h.pin(&pinned, true);
        let view = build(&state(&h));
        assert_eq!(
            sections(&view),
            ["PINNED", "RECENT", "SHORTCUT", "WHAT CLIPS KEEPS"]
        );
        let all = rows(&view);
        let first = all
            .iter()
            .find(|r| r.title == "first")
            .cloned()
            .unwrap_or_else(|| all[0].clone());
        let ids: Vec<&str> = first.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["copy", "pin", "delete"]);
        assert_eq!(first.detail, "Text · notepad · 14:30");
        let kept = all
            .iter()
            .find(|r| r.title == "kept")
            .map(|r| r.actions.clone());
        assert_eq!(
            kept.map(|a| a.iter().map(|x| x.id.clone()).collect::<Vec<_>>()),
            Some(vec!["copy".to_string(), "unpin".into(), "delete".into()])
        );
    }

    #[test]
    fn pause_ignore_next_and_clear_are_on_the_page() {
        let mut h = History::default();
        h.add(&Capture::text("x", None), now().with_timezone(&chrono::Utc));
        let mut s = state(&h);
        let view = build(&s);
        assert!(
            has_button(&view, "pause")
                && has_button(&view, "ignore-next")
                && has_button(&view, "clear")
        );
        s.paused_until = Some(now() + Duration::minutes(15));
        s.ignore_next = true;
        s.confirming_clear = true;
        let view = build(&s);
        assert!(has_button(&view, "resume") && !has_button(&view, "pause"));
        assert!(
            view.iter()
                .any(|w| matches!(w, Widget::Note { text, .. } if text == "Paused until 14:45."))
        );
        assert!(has_button(&view, "clear-confirm") && has_button(&view, "clear-cancel"));
        // Not watching: no pause, and the switch says so.
        s.settings.watching = false;
        let view = build(&s);
        assert!(!has_button(&view, "pause") && !has_button(&view, "resume"));
    }

    #[test]
    fn the_shortcut_row_says_if_windows_refused_it() {
        let h = History::default();
        let mut s = state(&h);
        s.shortcut = ShortcutState::Refused;
        let row = rows(&build(&s))
            .into_iter()
            .find(|r| r.title == "Open the list");
        assert_eq!(row.as_ref().map(|r| r.lamp), Some(Lamp::Error));
        assert!(row.is_some_and(|r| r.detail.contains("another app")));
        s.settings.shortcut = None;
        s.shortcut = ShortcutState::Ready;
        let row = rows(&build(&s))
            .into_iter()
            .find(|r| r.title == "Open the list");
        assert_eq!(
            row.map(|r| (r.lamp, r.trailing)),
            Some((Lamp::Off, "none".to_string()))
        );
    }

    #[test]
    fn a_shortcut_nothing_holds_yet_says_so() {
        let h = History::default();
        let mut s = state(&h);
        s.shortcut = ShortcutState::NotYet;
        let row = rows(&build(&s))
            .into_iter()
            .find(|r| r.title == "Open the list");
        assert_eq!(row.as_ref().map(|r| r.lamp), Some(Lamp::Off));
        assert!(row.is_some_and(|r| r.detail.starts_with("Not ready yet")));
    }

    #[test]
    fn the_closing_note_says_uninstalling_deletes_the_history() {
        let h = History::default();
        let view = build(&state(&h));
        assert!(view.iter().any(|w| matches!(w,
            Widget::Note { text, .. } if text.contains("uninstalling deletes it"))));
    }

    #[test]
    fn titles_are_cut_between_characters_as_people_see_them() {
        let titled = |text: &str| {
            let mut h = History::default();
            h.add(&Capture::text(text, None), chrono::Utc::now());
            rows(&build(&state(&h)))
                .into_iter()
                .next()
                .map(|r| r.title)
                .unwrap_or_default()
        };
        // Thai with its marks: 30 characters are three times as many code points, and kept whole.
        assert_eq!(titled(&"ที่".repeat(30)), "ที่".repeat(30));
        // 80 is kept whole; 85 is cut to 79 and an ellipsis, never through a mark.
        assert_eq!(titled(&"ที่".repeat(80)), "ที่".repeat(80));
        assert_eq!(titled(&"ที่".repeat(85)), format!("{}…", "ที่".repeat(79)));
        assert_eq!(titled(&"ทำ".repeat(85)), format!("{}…", "ทำ".repeat(79)));
        // Emoji made of several: a family, a skin tone, a flag.
        for one in ["👩‍💻", "👍🏽", "🇹🇭", "❤️"] {
            assert_eq!(
                titled(&one.repeat(85)),
                format!("{}…", one.repeat(79)),
                "{one}"
            );
        }
    }

    #[test]
    fn a_list_that_couldnt_start_says_so() {
        let h = History::default();
        let mut s = state(&h);
        s.shortcut = ShortcutState::Broken;
        let row = rows(&build(&s))
            .into_iter()
            .find(|r| r.title == "Open the list");
        assert_eq!(row.as_ref().map(|r| r.lamp), Some(Lamp::Error));
        assert!(row.is_some_and(|r| r.detail.contains("couldn't start")));
    }

    #[test]
    fn the_badge_counts_what_is_kept() {
        let mut h = History::default();
        assert_eq!(badge(&h), (0, "Clips".to_string()));
        h.add(&Capture::text("a", None), chrono::Utc::now());
        assert_eq!(badge(&h), (0, "Clips · 1 item".to_string()));
        h.add(&Capture::text("b", None), chrono::Utc::now());
        assert_eq!(badge(&h).1, "Clips · 2 items");
    }
}
