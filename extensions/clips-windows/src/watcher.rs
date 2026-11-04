//! Watching the clipboard (ADR-005 C1, R21, R22), in the Mac's order of checks. On Windows the clipboard says when it
//! changes, so `tick` runs once per change rather than twice a second; each time it reads the sequence number first.
//! Only when that changed does it read the formats' names and markers, and only when those pass does it read the
//! contents. Ported from the Mac's `Watcher.swift`.

use crate::filters::{Marks, allows, default_ignored_apps};
pub use crate::history::{Capture, Kind};
use chrono::{DateTime, Utc};
use std::collections::{BTreeSet, HashSet};

/// The clipboard formats Clips knows, by the names its `Board` gives them.
pub mod clip {
    pub use crate::filters::EXCLUDE_FROM_MONITORING;
    pub const TEXT: &str = "CF_UNICODETEXT";
    pub const DIB: &str = "CF_DIB";
    pub const FILES: &str = "CF_HDROP";
    pub const PNG: &str = "PNG";
    pub const RTF: &str = "Rich Text Format";
    pub const HTML: &str = "HTML Format";
    /// DWORD markers a password manager sets to 0.
    pub const CAN_INCLUDE_IN_HISTORY: &str = "CanIncludeInClipboardHistory";
    pub const CAN_UPLOAD_TO_CLOUD: &str = "CanUploadToCloudClipboard";

    /// The format an item's `data_type` was read from.
    pub fn of(data_type: &str) -> Option<&'static str> {
        match data_type {
            "png" => Some(PNG),
            "dib" => Some(DIB),
            "rtf" => Some(RTF),
            "html" => Some(HTML),
            _ => None,
        }
    }
}

/// What Clips puts on the clipboard: an item, back, to copy or paste it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Put {
    pub text: Option<String>,
    /// A format, by name (`clip::PNG`, …), and its bytes.
    pub data: Option<(String, Vec<u8>)>,
    pub files: Vec<String>,
}

/// The clipboard, as the watcher reads it; tests stand in for Windows'.
pub trait Board {
    /// Windows' clipboard sequence number: it changes with every copy.
    fn sequence(&self) -> u32;
    /// The names of the formats on it now (standard ones as `CF_…`).
    fn formats(&self) -> Vec<String>;
    /// A registered DWORD marker's value, if it is there.
    fn marker(&self, name: &str) -> Option<u32>;
    fn text(&self) -> Option<String>;
    fn data(&self, format: &str) -> Option<Vec<u8>>;
    fn files(&self) -> Vec<String>;
    /// Replaces what is on the clipboard; the sequence number after, or `None` if Windows refused (another app has
    /// the clipboard open).
    fn put(&mut self, put: &Put) -> Option<u32>;
}

pub struct Watcher<B: Board> {
    board: B,
    /// The program in front, by name.
    front: Box<dyn Fn() -> Option<String> + Send>,
    last: u32,
    /// Sequence numbers of Clips' own puts (to paste a pick): not new copies.
    own: HashSet<u32>,
    pub ignored_apps: BTreeSet<String>,
    /// Keep nothing until then.
    pub paused_until: Option<DateTime<Utc>>,
    /// Ignore the next copy, once.
    pub ignore_next: bool,
}

impl<B: Board> Watcher<B> {
    pub fn new(board: B, front: Box<dyn Fn() -> Option<String> + Send>) -> Self {
        let last = board.sequence();
        Self {
            board,
            front,
            last,
            own: HashSet::new(),
            ignored_apps: default_ignored_apps(),
            paused_until: None,
            ignore_next: false,
        }
    }

    pub fn board_mut(&mut self) -> &mut B {
        &mut self.board
    }

    /// Whatever is on the clipboard now isn't new (watching starts, or starts again).
    pub fn skip_current(&mut self) {
        self.last = self.board.sequence();
    }

    /// Clips put this on the clipboard itself (to paste a pick).
    pub fn own(&mut self, sequence: u32) {
        self.own.insert(sequence);
    }

    pub fn paused(&self, now: DateTime<Utc>) -> bool {
        self.paused_until.is_some_and(|until| now < until)
    }

    /// One look: a capture if something new, allowed and readable was copied.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Option<Capture> {
        let sequence = self.board.sequence();
        if sequence == self.last {
            return None;
        }
        self.last = sequence;
        if self.own.remove(&sequence) || self.paused(now) {
            return None;
        }
        if self.ignore_next {
            self.ignore_next = false;
            return None;
        }
        let marks = Marks {
            names: self.board.formats(),
            no_history: self.board.marker(clip::CAN_INCLUDE_IN_HISTORY) == Some(0),
            no_cloud: self.board.marker(clip::CAN_UPLOAD_TO_CLOUD) == Some(0),
        };
        let app = (self.front)();
        if !allows(&marks, app.as_deref(), &self.ignored_apps) {
            return None;
        }
        self.read(&marks.names, app)
    }

    /// The contents, read only now: files, an image, rich text with its plain text, or plain text.
    fn read(&self, formats: &[String], source: Option<String>) -> Option<Capture> {
        let has = |f: &str| formats.iter().any(|n| n == f);
        let capture = |kind, text: String, data, data_type: Option<&str>, files| Capture {
            kind,
            text,
            data,
            data_type: data_type.map(str::to_string),
            files,
            source: source.clone(),
        };
        if has(clip::FILES) {
            let files = self.board.files();
            if !files.is_empty() {
                return Some(capture(Kind::Files, files.join("\n"), None, None, files));
            }
        }
        for (format, kind) in [(clip::PNG, "png"), (clip::DIB, "dib")] {
            if has(format)
                && let Some(data) = self.board.data(format).filter(|d| !d.is_empty())
            {
                return Some(capture(
                    Kind::Image,
                    String::new(),
                    Some(data),
                    Some(kind),
                    Vec::new(),
                ));
            }
        }
        let text = if has(clip::TEXT) {
            self.board.text()
        } else {
            None
        };
        let text = text.filter(|t| !t.is_empty());
        for (format, kind) in [(clip::RTF, "rtf"), (clip::HTML, "html")] {
            if has(format)
                && let Some(text) = &text
                && let Some(data) = self.board.data(format).filter(|d| !d.is_empty())
            {
                return Some(capture(
                    Kind::Rich,
                    text.clone(),
                    Some(data),
                    Some(kind),
                    Vec::new(),
                ));
            }
        }
        text.map(|t| capture(Kind::Text, t, None, None, Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBoard;
    use std::rc::Rc;

    fn watcher(board: FakeBoard, front: &'static str) -> Watcher<FakeBoard> {
        Watcher::new(board, Box::new(move || Some(front.to_string())))
    }

    #[test]
    fn secret_copies_are_never_kept() {
        let mut w = watcher(FakeBoard::default(), "notepad.exe");
        w.board_mut().copy_text("hunter2");
        w.board_mut()
            .formats
            .push(clip::EXCLUDE_FROM_MONITORING.into());
        assert_eq!(w.tick(Utc::now()), None);
        w.board_mut().copy_text("hunter3");
        w.board_mut().no_history = true;
        assert_eq!(w.tick(Utc::now()), None);
        w.board_mut().copy_text("fine");
        assert!(w.tick(Utc::now()).is_some());
    }

    #[test]
    fn copies_in_ignored_apps_are_never_kept() {
        let mut w = watcher(FakeBoard::default(), "Bitwarden.exe");
        w.board_mut().copy_text("hunter2");
        assert_eq!(w.tick(Utc::now()), None);
    }

    #[test]
    fn contents_are_read_only_after_the_formats_pass() {
        let board = FakeBoard::default();
        let contents = Rc::clone(&board.content_reads);
        let mut w = watcher(board, "notepad.exe");
        w.board_mut().copy_text("hunter2");
        w.board_mut()
            .formats
            .push(clip::EXCLUDE_FROM_MONITORING.into());
        assert_eq!(w.tick(Utc::now()), None);
        assert_eq!(contents.get(), 0, "a secret's contents were read");
    }

    #[test]
    fn idle_is_one_sequence_read() {
        let board = FakeBoard::default();
        let (seq, formats, contents) = (
            Rc::clone(&board.sequence_reads),
            Rc::clone(&board.format_reads),
            Rc::clone(&board.content_reads),
        );
        let mut w = watcher(board, "notepad.exe");
        seq.set(0);
        for _ in 0..100 {
            assert_eq!(w.tick(Utc::now()), None);
        }
        assert_eq!((seq.get(), formats.get(), contents.get()), (100, 0, 0));
    }

    #[test]
    fn each_kind_is_read() {
        let mut w = watcher(FakeBoard::default(), "explorer.exe");
        let b = w.board_mut();
        b.sequence += 1;
        b.formats = vec![clip::FILES.into(), clip::TEXT.into()];
        b.files = vec![r"C:\a\one.txt".into()];
        let c = w.tick(Utc::now());
        assert_eq!(c.as_ref().map(|c| c.kind), Some(Kind::Files));
        assert_eq!(c.map(|c| c.source), Some(Some("explorer.exe".into())));

        let b = w.board_mut();
        b.sequence += 1;
        b.formats = vec![clip::DIB.into(), clip::PNG.into()];
        b.files.clear();
        b.data = vec![
            (clip::PNG.into(), vec![137, 80]),
            (clip::DIB.into(), vec![1]),
        ];
        let c = w.tick(Utc::now());
        assert_eq!(
            c.as_ref().map(|c| (c.kind, c.data_type.clone())),
            Some((Kind::Image, Some("png".into())))
        );

        let b = w.board_mut();
        b.sequence += 1;
        b.formats = vec![clip::TEXT.into(), clip::RTF.into()];
        b.text = Some("bold".into());
        b.data = vec![(clip::RTF.into(), b"{\\rtf1 bold}".to_vec())];
        let c = w.tick(Utc::now());
        assert_eq!(
            c.as_ref().map(|c| (c.kind, c.text.clone())),
            Some((Kind::Rich, "bold".into()))
        );

        w.board_mut().copy_text("plain");
        let c = w.tick(Utc::now());
        assert_eq!(c.map(|c| c.kind), Some(Kind::Text));
    }

    #[test]
    fn own_puts_pauses_and_ignore_next_keep_nothing() {
        let mut w = watcher(FakeBoard::default(), "notepad.exe");
        // Clips' own put (to paste a pick).
        w.board_mut().copy_text("picked");
        let seq = w.board_mut().sequence;
        w.own(seq);
        assert_eq!(w.tick(Utc::now()), None);
        // Paused.
        let now = Utc::now();
        w.paused_until = Some(now + chrono::Duration::minutes(15));
        w.board_mut().copy_text("while paused");
        assert_eq!(w.tick(now), None);
        w.paused_until = None;
        // Ignore next, once.
        w.ignore_next = true;
        w.board_mut().copy_text("ignored");
        assert_eq!(w.tick(now), None);
        w.board_mut().copy_text("kept");
        assert!(w.tick(now).is_some());
        // What was there when watching started isn't new.
        w.board_mut().copy_text("while off");
        w.skip_current();
        assert_eq!(w.tick(now), None);
    }
}
