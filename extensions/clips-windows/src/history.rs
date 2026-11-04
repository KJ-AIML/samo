//! The history (ADR-005 C1): newest first, a repeat moving to the top and counting once more, pins kept whatever the
//! limit. Only what's needed to list and search is in memory; rich text and images are files (R21). Ported from the
//! Mac's `History.swift`.

use chrono::{DateTime, Utc};
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};

/// How many unpinned items are kept unless you choose otherwise.
pub const DEFAULT_LIMIT: usize = 200;
/// Text longer than this is kept whole in a file; the list holds its first 64 KB.
pub const INLINE_TEXT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Rich,
    Image,
    Files,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Rich => "rich",
            Kind::Image => "image",
            Kind::Files => "files",
        }
    }
}

/// What was copied, as the watcher read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub kind: Kind,
    /// Plain text, or the files' paths, one a line.
    pub text: String,
    /// Rich text or an image, as copied.
    pub data: Option<Vec<u8>>,
    /// `png`, `dib`, `rtf` or `html`.
    pub data_type: Option<String>,
    pub files: Vec<String>,
    /// The app in front when it was noticed (its program's name).
    pub source: Option<String>,
}

impl Capture {
    pub fn text(text: &str, source: Option<&str>) -> Self {
        Self {
            kind: Kind::Text,
            text: text.into(),
            data: None,
            data_type: None,
            files: Vec::new(),
            source: source.map(str::to_string),
        }
    }

    /// What makes two copies the same: a SHA-256 of the kind, the text and the data, hashed as they are (a large
    /// copy isn't copied again to be hashed).
    fn fingerprint(&self) -> String {
        let mut hash = Context::new(&SHA256);
        hash.update(self.kind.name().as_bytes());
        hash.update(self.text.as_bytes());
        if let Some(data) = &self.data {
            hash.update(data);
        }
        hash.finish()
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    /// Plain text (its first 64 KB if longer: the rest is in `text_file`), or files' paths, one a line.
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_file: Option<String>,
    /// Rich text or an image, in `items/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    pub bytes: u64,
    pub first: DateTime<Utc>,
    pub last: DateTime<Utc>,
    pub copies: u32,
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub fingerprint: String,
}

/// A path's last part, whichever slash it uses.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

impl Item {
    /// The first line, as a list row shows it.
    pub fn title(&self) -> String {
        match self.kind {
            Kind::Image => "Image".into(),
            Kind::Files => {
                let names: Vec<&str> = self.files.iter().map(|f| file_name(f)).collect();
                match names.as_slice() {
                    [one] => (*one).to_string(),
                    _ => format!("{} files: {}", names.len(), names.join(", ")),
                }
            }
            Kind::Text | Kind::Rich => self
                .text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or_default()
                .to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    /// How many unpinned items are kept.
    pub limit: usize,
    /// Pinned items first (in the order pinned), then the rest, newest first.
    items: Vec<Item>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            items: Vec::new(),
        }
    }
}

/// What adding a capture did: the item (new or repeated), whether it is new, and what fell off the end.
#[derive(Debug, Clone)]
pub struct Added {
    pub item: Item,
    pub is_new: bool,
    pub dropped: Vec<Item>,
}

impl History {
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn pinned(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| i.pinned).collect()
    }

    pub fn recent(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| !i.pinned).collect()
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Adds a capture copied at `now`. A repeat moves to the top of its group and counts once more.
    pub fn add(&mut self, capture: &Capture, now: DateTime<Utc>) -> Added {
        let print = capture.fingerprint();
        if let Some(i) = self.items.iter().position(|it| it.fingerprint == print) {
            let mut item = self.items.remove(i);
            item.last = now;
            item.copies = item.copies.saturating_add(1);
            item.source = capture.source.clone().or(item.source);
            if item.pinned {
                // A pin stays where it was pinned.
                self.items.insert(i, item.clone());
            } else {
                self.insert(item.clone());
            }
            return Added {
                item,
                is_new: false,
                dropped: Vec::new(),
            };
        }
        let bytes = capture.data.as_ref().map_or(0, Vec::len) + capture.text.len();
        let item = Item {
            id: uuid::Uuid::new_v4().to_string(),
            kind: capture.kind,
            text: History::prefix(&capture.text, INLINE_TEXT),
            text_file: None,
            data_file: None,
            data_type: capture.data_type.clone(),
            files: capture.files.clone(),
            bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
            first: now,
            last: now,
            copies: 1,
            pinned: false,
            source: capture.source.clone(),
            fingerprint: print,
        };
        self.insert(item.clone());
        Added {
            item,
            is_new: true,
            dropped: self.trim(),
        }
    }

    /// At most `bytes` of `text`, cut between characters (never through a Thai character, or an emoji).
    pub fn prefix(text: &str, bytes: usize) -> String {
        if text.len() <= bytes {
            return text.to_string();
        }
        let mut end = bytes;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.get(..end).unwrap_or_default().to_string()
    }

    /// Sets where an item's contents were written.
    pub fn set_files(&mut self, id: &str, text: Option<String>, data: Option<String>) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.text_file = text;
            item.data_file = data;
        }
    }

    /// At the top of the unpinned items, just below the pins.
    fn insert(&mut self, item: Item) {
        let at = self
            .items
            .iter()
            .position(|i| !i.pinned)
            .unwrap_or(self.items.len());
        self.items.insert(at, item);
    }

    /// Drops the oldest unpinned items beyond the limit; returns them, so their files can go too.
    pub fn trim(&mut self) -> Vec<Item> {
        let mut dropped = Vec::new();
        while self.recent().len() > self.limit {
            match self.items.iter().rposition(|i| !i.pinned) {
                Some(i) => dropped.push(self.items.remove(i)),
                None => break,
            }
        }
        dropped
    }

    /// Sets how many unpinned items are kept; returns what no longer fits.
    pub fn set_limit(&mut self, limit: usize) -> Vec<Item> {
        self.limit = limit;
        self.trim()
    }

    /// Pins or unpins; a pin goes to the end of the pins, an unpin to the top of the rest.
    pub fn pin(&mut self, id: &str, on: bool) {
        let Some(i) = self
            .items
            .iter()
            .position(|it| it.id == id && it.pinned != on)
        else {
            return;
        };
        let mut item = self.items.remove(i);
        item.pinned = on;
        self.insert(item);
    }

    pub fn delete(&mut self, id: &str) -> Option<Item> {
        let i = self.items.iter().position(|it| it.id == id)?;
        Some(self.items.remove(i))
    }

    /// Clears everything but the pins; returns what went.
    pub fn clear(&mut self) -> Vec<Item> {
        let (gone, kept): (Vec<Item>, Vec<Item>) = self.items.drain(..).partition(|i| !i.pinned);
        self.items = kept;
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use ring::digest::digest;

    fn text(t: &str) -> Capture {
        Capture::text(t, Some("notepad.exe"))
    }

    #[test]
    fn repeats_move_to_the_top_and_count() {
        let mut h = History::default();
        let t0 = Utc
            .with_ymd_and_hms(2026, 11, 5, 9, 0, 0)
            .single()
            .unwrap_or_default();
        h.add(&text("a"), t0);
        h.add(&text("b"), t0 + Duration::seconds(1));
        let again = h.add(&text("a"), t0 + Duration::seconds(2));
        assert!(!again.is_new);
        let titles: Vec<String> = h.items().iter().map(Item::title).collect();
        assert_eq!(titles, ["a", "b"]);
        assert_eq!(h.items()[0].copies, 2);
        assert_eq!(h.items()[0].last, t0 + Duration::seconds(2));
        assert_eq!(h.items()[0].first, t0);
    }

    #[test]
    fn the_limit_drops_the_oldest_but_never_a_pin() {
        let mut h = History {
            limit: 2,
            ..History::default()
        };
        let t0 = Utc::now();
        let first = h.add(&text("one"), t0).item.id.clone();
        h.pin(&first, true);
        h.add(&text("two"), t0);
        h.add(&text("three"), t0);
        let added = h.add(&text("four"), t0);
        assert_eq!(added.dropped.len(), 1);
        assert_eq!(added.dropped[0].title(), "two");
        let titles: Vec<String> = h.items().iter().map(Item::title).collect();
        assert_eq!(titles, ["one", "four", "three"]);
        // A repeat of a pin stays where it was pinned.
        h.add(&text("one"), t0);
        assert_eq!(h.items()[0].title(), "one");
        assert!(h.items()[0].pinned);
        // Clear keeps pins; unpinning puts it at the top of the rest.
        assert_eq!(h.clear().len(), 2);
        h.pin(&first, false);
        assert_eq!(h.recent().len(), 1);
    }

    #[test]
    fn long_text_is_cut_between_characters() {
        // Thai is 3 bytes a character: a cut at an odd byte must step back to a character's start.
        let thai = "ก".repeat(30_000);
        let cut = History::prefix(&thai, INLINE_TEXT);
        assert!(cut.len() <= INLINE_TEXT && cut.len().is_multiple_of(3));
        assert!(thai.starts_with(&cut));
        assert_eq!(History::prefix("short", 10), "short");
    }

    #[test]
    fn titles_say_what_was_copied() {
        let t = Utc::now();
        let mut h = History::default();
        h.add(&text("  first line \nsecond"), t);
        h.add(
            &Capture {
                kind: Kind::Files,
                text: r"C:\a\one.txt".into(),
                data: None,
                data_type: None,
                files: vec![r"C:\a\one.txt".into(), r"C:\b\two.png".into()],
                source: None,
            },
            t,
        );
        h.add(
            &Capture {
                kind: Kind::Image,
                text: String::new(),
                data: Some(vec![1, 2, 3]),
                data_type: Some("png".into()),
                files: Vec::new(),
                source: None,
            },
            t,
        );
        let titles: Vec<String> = h.items().iter().map(Item::title).collect();
        assert_eq!(titles, ["Image", "2 files: one.txt, two.png", "first line"]);
    }

    #[test]
    fn a_title_skips_blank_lines() {
        let mut h = History::default();
        h.add(
            &text(
                "
   
	Hello there
second",
            ),
            Utc::now(),
        );
        assert_eq!(
            h.items().first().map(Item::title).as_deref(),
            Some("Hello there")
        );
    }

    #[test]
    fn a_fingerprint_is_the_sha256_of_kind_text_and_data() {
        let mut c = text("hello");
        c.data = Some(vec![1, 2, 3]);
        let mut whole = b"texthello".to_vec();
        whole.extend_from_slice(&[1, 2, 3]);
        let expected: String = digest(&SHA256, &whole)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(c.fingerprint(), expected);
    }
}
