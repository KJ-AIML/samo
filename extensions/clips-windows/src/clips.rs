//! Clips, put together, on every OS: the history and its files, the settings, the watcher and the page. Samo's
//! messages and each look at the clipboard come through here. `serve` does the talking, and Windows' clipboard,
//! shortcut and list plug in through `Board` (ADR-005 C1–C4). Ported from the Mac's `ClipsApp.swift`.

use crate::history::{History, Kind};
use crate::page::{self, PageState, ShortcutState, shortcut_choices};
use crate::settings::Settings;
use crate::store::Store;
use crate::watcher::{Board, Put, Watcher, clip};
use chrono::{DateTime, Local, Utc};
use samo_ext_protocol::{HelperMsg, HostMsg, PopupKind, Widget};
use std::fs;
use std::path::{Path, PathBuf};

/// The history's sizes the page offers.
const LIMITS: [usize; 3] = [50, 200, 500];
const SETTINGS: &str = "settings.json";
const PAUSE_MINUTES: i64 = 15;

pub struct Clips<B: Board> {
    dir: PathBuf,
    store: Store,
    history: History,
    settings: Settings,
    watcher: Watcher<B>,
    shortcut: ShortcutState,
    confirming_clear: bool,
    /// Why the last save failed, if it did.
    problem: Option<String>,
    sent_view: Option<Vec<Widget>>,
    sent_badge: Option<(u32, String)>,
    sent_problem: Option<String>,
}

impl<B: Board> Clips<B> {
    /// Clips with its saved history and settings in `dir`. What is on the clipboard now isn't a new copy.
    pub fn open(dir: &Path, board: B, front: Box<dyn Fn() -> Option<String> + Send>) -> Self {
        let settings = fs::read_to_string(dir.join(SETTINGS))
            .map(|text| Settings::parse(&text))
            .unwrap_or_default();
        Self {
            dir: dir.to_path_buf(),
            store: Store::new(dir),
            history: Store::new(dir).load(),
            settings,
            watcher: Watcher::new(board, front),
            shortcut: ShortcutState::Ready,
            confirming_clear: false,
            problem: None,
            sent_view: None,
            sent_badge: None,
            sent_problem: None,
        }
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn board_mut(&mut self) -> &mut B {
        self.watcher.board_mut()
    }

    /// Whether Windows took the shortcut (told by whoever registers it).
    pub fn set_shortcut_state(&mut self, state: ShortcutState) {
        self.shortcut = state;
    }

    /// One of Samo's messages; what to tell Samo at once (a pop-up), if anything. The page follows in `updates`.
    pub fn handle(&mut self, msg: &HostMsg, now: DateTime<Utc>) -> Vec<HelperMsg> {
        match msg {
            HostMsg::Setting { key, value } if key == "watching" => {
                if *value && !self.settings.watching {
                    self.watcher.skip_current();
                }
                self.settings.watching = *value;
                self.save_settings();
            }
            HostMsg::Action { id, row } => return self.action(id, row.as_deref(), now),
            HostMsg::Choice { key, value } => self.choice(key, value),
            _ => {}
        }
        Vec::new()
    }

    fn action(&mut self, id: &str, row: Option<&str>, now: DateTime<Utc>) -> Vec<HelperMsg> {
        if id != "clear" {
            self.confirming_clear = false;
        }
        if let Some(row) = row {
            match id {
                "copy" if self.history.item(row).is_some() && !self.put(row, false) => {
                    return vec![HelperMsg::Popup {
                        key: "copy".into(),
                        kind: PopupKind::Info,
                        title: "Couldn't copy: another app is using the clipboard".into(),
                        body: "Try again in a moment.".into(),
                    }];
                }
                "pin" | "unpin" => {
                    self.history.pin(row, id == "pin");
                    self.save();
                }
                "delete" => self.delete(row),
                _ => {}
            }
            return Vec::new();
        }
        match id {
            "pause" => {
                self.watcher.paused_until = Some(now + chrono::Duration::minutes(PAUSE_MINUTES));
            }
            "resume" => self.watcher.paused_until = None,
            "ignore-next" => self.watcher.ignore_next = true,
            "clear" => self.confirming_clear = true,
            "clear-confirm" => {
                for gone in self.history.clear() {
                    self.store.delete(&gone);
                }
                self.save();
            }
            _ => {}
        }
        Vec::new()
    }

    fn choice(&mut self, key: &str, value: &str) {
        match key {
            "limit" => {
                if let Ok(limit) = value.parse::<usize>()
                    && LIMITS.contains(&limit)
                {
                    for gone in self.history.set_limit(limit) {
                        self.store.delete(&gone);
                    }
                    self.save();
                }
            }
            "shortcut" => {
                if let Some((_, shortcut)) =
                    shortcut_choices().into_iter().find(|(id, _)| *id == value)
                {
                    self.settings.shortcut = shortcut;
                    self.shortcut = ShortcutState::Ready;
                    self.save_settings();
                }
            }
            _ => {}
        }
    }

    /// Puts an item back on the clipboard as Clips' own (not a new copy); `plain` leaves rich text's formatting out.
    /// False if the item is gone or Windows refused.
    pub fn put(&mut self, id: &str, plain: bool) -> bool {
        let Some(item) = self.history.item(id) else {
            return false;
        };
        let text = match item.kind {
            Kind::Image | Kind::Files => None,
            Kind::Text | Kind::Rich => Some(self.store.full_text(item)),
        };
        let data = if plain && item.kind == Kind::Rich {
            None
        } else {
            let format = item.data_type.as_deref().and_then(clip::of);
            let bytes = item.data_file.as_deref().and_then(|f| self.store.read(f));
            format.zip(bytes).map(|(f, b)| (f.to_string(), b))
        };
        let put = Put {
            text,
            data,
            files: item.files.clone(),
        };
        match self.watcher.board_mut().put(&put) {
            Some(sequence) => {
                self.watcher.own(sequence);
                true
            }
            None => false,
        }
    }

    pub fn delete(&mut self, id: &str) {
        if let Some(gone) = self.history.delete(id) {
            self.store.delete(&gone);
            self.save();
        }
    }

    /// One look at the clipboard; true if a copy was kept.
    pub fn tick(&mut self, now: DateTime<Utc>) -> bool {
        if !self.settings.watching {
            return false;
        }
        let Some(capture) = self.watcher.tick(now) else {
            return false;
        };
        let added = self.history.add(&capture, now);
        let kept = self.store.keep(&capture, &added, &mut self.history);
        for gone in &added.dropped {
            self.store.delete(gone);
        }
        let saved = self.store.save(&self.history);
        self.problem = kept.and(saved).err();
        true
    }

    fn save(&mut self) {
        self.problem = self.store.save(&self.history).err();
    }

    fn save_settings(&mut self) {
        let saved = fs::create_dir_all(&self.dir)
            .map_err(|e| format!("couldn't make Clips' folder: {e}"))
            .and_then(|()| {
                Store::write_atomic(&self.dir.join(SETTINGS), self.settings.to_json().as_bytes())
            });
        self.problem = saved.err();
    }

    fn page(&self, now: DateTime<Utc>) -> Vec<Widget> {
        page::build(&PageState {
            history: &self.history,
            settings: self.settings.clone(),
            shortcut: self.shortcut,
            paused_until: self
                .watcher
                .paused_until
                .filter(|until| *until > now)
                .map(|until| until.with_timezone(&Local)),
            ignore_next: self.watcher.ignore_next,
            confirming_clear: self.confirming_clear,
            now: now.with_timezone(&Local),
        })
    }

    /// What Samo hasn't been told yet: the page, the badge, and whether saving works; all of it when `force`.
    pub fn updates(&mut self, now: DateTime<Utc>, force: bool) -> Vec<HelperMsg> {
        let mut out = Vec::new();
        if self.problem != self.sent_problem || (force && self.problem.is_some()) {
            out.push(HelperMsg::Health {
                ok: self.problem.is_none(),
                message: self
                    .problem
                    .as_ref()
                    .map_or_else(String::new, |p| format!("Clips {p}")),
            });
            self.sent_problem.clone_from(&self.problem);
        }
        let view = self.page(now);
        if force || self.sent_view.as_ref() != Some(&view) {
            out.push(HelperMsg::View {
                sections: view.clone(),
            });
            self.sent_view = Some(view);
        }
        let badge = page::badge(&self.history);
        if force || self.sent_badge.as_ref() != Some(&badge) {
            out.push(HelperMsg::Badge {
                count: badge.0,
                summary: badge.1.clone(),
                meter: None,
            });
            self.sent_badge = Some(badge);
        }
        out
    }

    /// Samo is stopping Clips: the history is saved; the watch stops with the process.
    pub fn shutdown(&mut self) {
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBoard;
    use crate::settings::Shortcut;
    use crate::watcher::clip;
    use samo_ext_protocol::{Lamp, Widget};

    fn open(dir: &Path) -> Clips<FakeBoard> {
        Clips::open(
            dir,
            FakeBoard::default(),
            Box::new(|| Some("notepad.exe".into())),
        )
    }

    fn action(id: &str, row: Option<&str>) -> HostMsg {
        HostMsg::Action {
            id: id.into(),
            row: row.map(str::to_string),
        }
    }

    fn view(msgs: &[HelperMsg]) -> Option<&[Widget]> {
        msgs.iter().find_map(|m| match m {
            HelperMsg::View { sections } => Some(sections.as_slice()),
            _ => None,
        })
    }

    fn badge(msgs: &[HelperMsg]) -> Option<&str> {
        msgs.iter().find_map(|m| match m {
            HelperMsg::Badge { summary, .. } => Some(summary.as_str()),
            _ => None,
        })
    }

    fn titles(view: &[Widget]) -> Vec<String> {
        view.iter()
            .filter_map(|w| match w {
                Widget::Rows { items } => Some(items),
                _ => None,
            })
            .flatten()
            .map(|r| r.title.clone())
            .collect()
    }

    fn buttons(view: &[Widget]) -> Vec<String> {
        view.iter()
            .filter_map(|w| match w {
                Widget::Button { action, .. } => Some(action.clone()),
                _ => None,
            })
            .collect()
    }

    fn copy(c: &mut Clips<FakeBoard>, text: &str, now: DateTime<Utc>) -> bool {
        c.board_mut().copy_text(text);
        c.tick(now)
    }

    fn id_of(c: &Clips<FakeBoard>, text: &str) -> String {
        c.history()
            .items()
            .iter()
            .find(|i| i.text == text)
            .map(|i| i.id.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_copy_is_kept_saved_and_listed() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        assert!(copy(&mut c, "hello", now));
        let msgs = c.updates(now, false);
        assert!(view(&msgs).is_some_and(|v| titles(v).contains(&"hello".to_string())));
        assert_eq!(badge(&msgs), Some("Clips · 1 item"));
        assert_eq!(open(tmp.path()).history().items().len(), 1, "not saved");
        Ok(())
    }

    #[test]
    fn the_page_and_badge_are_sent_only_when_they_change() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        let first = c.updates(now, false);
        assert!(view(&first).is_some() && badge(&first).is_some());
        assert!(c.updates(now, false).is_empty());
        assert_eq!(c.updates(now, true).len(), 2);
        Ok(())
    }

    #[test]
    fn pause_keeps_nothing_until_resumed() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        c.handle(&action("pause", None), now);
        assert!(!copy(&mut c, "while paused", now));
        let msgs = c.updates(now, false);
        assert!(view(&msgs).is_some_and(|v| buttons(v).contains(&"resume".to_string())));
        assert!(
            !copy(&mut c, "still paused", now + chrono::Duration::minutes(14)),
            "a pause is 15 minutes"
        );
        c.handle(&action("resume", None), now);
        assert!(copy(&mut c, "after", now));
        Ok(())
    }

    #[test]
    fn ignore_next_skips_one_copy() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        c.handle(&action("ignore-next", None), now);
        assert!(!copy(&mut c, "secret", now));
        assert!(copy(&mut c, "kept", now));
        Ok(())
    }

    #[test]
    fn watching_off_keeps_nothing_and_is_remembered() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        c.handle(
            &HostMsg::Setting {
                key: "watching".into(),
                value: false,
            },
            now,
        );
        assert!(!copy(&mut c, "while off", now));
        assert!(!open(tmp.path()).settings().watching, "not saved");
        c.handle(
            &HostMsg::Setting {
                key: "watching".into(),
                value: true,
            },
            now,
        );
        assert!(!c.tick(now), "a copy made while off was picked up");
        assert!(copy(&mut c, "on again", now));
        Ok(())
    }

    #[test]
    fn copy_puts_the_item_back_as_clips_own() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        copy(&mut c, "one", now);
        copy(&mut c, "two", now);
        let one = id_of(&c, "one");
        c.handle(&action("copy", Some(&one)), now);
        assert_eq!(
            c.board_mut().puts.last().and_then(|p| p.text.clone()),
            Some("one".into())
        );
        assert!(!c.tick(now), "Clips' own put was kept as a new copy");
        assert_eq!(c.history().item(&one).map(|i| i.copies), Some(1));
        Ok(())
    }

    #[test]
    fn rich_text_goes_back_with_its_format_unless_plain() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        let b = c.board_mut();
        b.sequence += 1;
        b.formats = vec![clip::TEXT.into(), clip::RTF.into()];
        b.text = Some("bold".into());
        b.data = vec![(clip::RTF.into(), b"{\\rtf1 bold}".to_vec())];
        assert!(c.tick(now));
        let id = id_of(&c, "bold");
        assert!(c.put(&id, false));
        assert_eq!(
            c.board_mut().puts.last().and_then(|p| p.data.clone()),
            Some((clip::RTF.to_string(), b"{\\rtf1 bold}".to_vec()))
        );
        assert!(c.put(&id, true));
        assert_eq!(
            c.board_mut()
                .puts
                .last()
                .map(|p| (p.text.clone(), p.data.clone())),
            Some((Some("bold".into()), None))
        );
        Ok(())
    }

    #[test]
    fn a_refused_put_says_so() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        copy(&mut c, "one", now);
        let one = id_of(&c, "one");
        c.board_mut().busy = true;
        let said = c.handle(&action("copy", Some(&one)), now);
        assert!(said.iter().any(|m| matches!(m,
            HelperMsg::Popup { title, .. } if title.contains("another app is using the clipboard"))));
        Ok(())
    }

    #[test]
    fn rows_pin_unpin_and_delete() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        copy(&mut c, "one", now);
        copy(&mut c, "two", now);
        let one = id_of(&c, "one");
        c.handle(&action("pin", Some(&one)), now);
        assert!(
            open(tmp.path())
                .history()
                .item(&one)
                .is_some_and(|i| i.pinned)
        );
        c.handle(&action("unpin", Some(&one)), now);
        assert!(c.history().item(&one).is_some_and(|i| !i.pinned));
        c.handle(&action("delete", Some(&one)), now);
        assert!(c.history().item(&one).is_none());
        assert_eq!(open(tmp.path()).history().items().len(), 1);
        Ok(())
    }

    #[test]
    fn clear_asks_first_and_keeps_pins() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        copy(&mut c, "one", now);
        copy(&mut c, "pinned", now);
        let pinned = id_of(&c, "pinned");
        c.handle(&action("pin", Some(&pinned)), now);
        c.handle(&action("clear", None), now);
        let asked = c.updates(now, true);
        assert!(view(&asked).is_some_and(|v| buttons(v).contains(&"clear-confirm".to_string())));
        assert_eq!(c.history().items().len(), 2, "cleared before confirming");
        c.handle(&action("clear-cancel", None), now);
        let back = c.updates(now, true);
        assert!(view(&back).is_some_and(|v| buttons(v).contains(&"clear".to_string())));
        c.handle(&action("clear", None), now);
        c.handle(&action("clear-confirm", None), now);
        assert_eq!(titles_of(&c), vec!["pinned".to_string()]);
        Ok(())
    }

    fn titles_of(c: &Clips<FakeBoard>) -> Vec<String> {
        c.history().items().iter().map(|i| i.text.clone()).collect()
    }

    #[test]
    fn the_limit_trims_and_ignores_other_values() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        for n in 0..60 {
            copy(&mut c, &format!("copy {n}"), now);
        }
        let choice = |value: &str| HostMsg::Choice {
            key: "limit".into(),
            value: value.into(),
        };
        c.handle(&choice("7"), now);
        assert_eq!(c.history().items().len(), 60);
        c.handle(&choice("50"), now);
        assert_eq!(c.history().items().len(), 50);
        assert_eq!(open(tmp.path()).history().limit, 50);
        Ok(())
    }

    #[test]
    fn the_shortcut_choice_is_saved() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        let choice = |value: &str| HostMsg::Choice {
            key: "shortcut".into(),
            value: value.into(),
        };
        c.handle(&choice("none"), now);
        assert_eq!(open(tmp.path()).settings().shortcut, None);
        c.handle(&choice("alt-shift-v"), now);
        let alt_shift = Some(Shortcut {
            ctrl: false,
            alt: true,
            shift: true,
            win: false,
            key: 'V',
        });
        assert_eq!(c.settings().shortcut, alt_shift);
        c.handle(&choice("bogus"), now);
        assert_eq!(open(tmp.path()).settings().shortcut, alt_shift);
        Ok(())
    }

    #[test]
    fn a_refused_shortcut_shows_on_the_page() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let mut c = open(tmp.path());
        c.set_shortcut_state(ShortcutState::Refused);
        let msgs = c.updates(now, false);
        let lamp = view(&msgs).and_then(|v| {
            v.iter().find_map(|w| match w {
                Widget::Rows { items } => items
                    .iter()
                    .find(|r| r.title == "Open the list")
                    .map(|r| r.lamp),
                _ => None,
            })
        });
        assert_eq!(lamp, Some(Lamp::Error));
        Ok(())
    }

    #[test]
    fn a_folder_it_cannot_write_says_so() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let now = Utc::now();
        // A file where Clips' folder should be.
        let blocked = tmp.path().join("data");
        std::fs::write(&blocked, "x").map_err(|e| e.to_string())?;
        let mut c = open(&blocked);
        assert!(copy(&mut c, "one", now));
        let msgs = c.updates(now, false);
        assert!(
            msgs.iter()
                .any(|m| matches!(m, HelperMsg::Health { ok: false, .. }))
        );
        Ok(())
    }
}
