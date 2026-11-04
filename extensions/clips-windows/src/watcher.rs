//! Watching the clipboard (ADR-005 C1, R21, R22), in the Mac's order of checks. Windows says when the clipboard
//! changes, and `tick` reads the sequence number first. Only when that changed does it open the clipboard, once, and
//! read the formats' names and markers; only when those pass does it read the contents, in the same open, so what
//! was checked is what is read. A marker it can't read counts as secret. Ported from the Mac's `Watcher.swift`.

use crate::filters::{Marks, allows, default_ignored_apps};
pub use crate::history::{Capture, Kind};
use chrono::{DateTime, Utc};
use std::collections::{BTreeSet, HashSet};

/// The largest copy Clips keeps, as Windows holds it (32 MiB); a larger one is left out, and Clips says so. Reading it
/// would take several times that while it is converted and kept, within Clips' 300 MB (R17).
pub const MAX_COPY: usize = 32 * 1024 * 1024;

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
    /// Formats, by name (`clip::PNG`, …), and their bytes.
    pub data: Vec<(String, Vec<u8>)>,
    pub files: Vec<String>,
}

impl Put {
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.data.is_empty() && self.files.is_empty()
    }
}

/// The clipboard, as the watcher reads it; tests stand in for Windows'.
pub trait Board {
    /// Windows' clipboard sequence number: it changes with every copy.
    fn sequence(&self) -> u32;
    /// Opens the clipboard for one look (the reads below, until `end`); false if another app has it open.
    fn begin(&self) -> bool;
    fn end(&self);
    /// The names of the formats on it now (standard ones as `CF_…`).
    fn formats(&self) -> Vec<String>;
    /// A registered DWORD marker's value, if it is there and can be read.
    fn marker(&self, name: &str) -> Option<u32>;
    /// A format's size in bytes, as Windows holds it.
    fn size(&self, format: &str) -> Option<usize>;
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
    /// The last look found the clipboard busy: look again soon.
    retry: bool,
    /// A copy left out for its size, not yet said.
    too_big: Option<usize>,
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
            retry: false,
            too_big: None,
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
        self.retry = false;
    }

    /// Clips put this on the clipboard itself (to paste a pick).
    pub fn own(&mut self, sequence: u32) {
        self.own.insert(sequence);
    }

    pub fn paused(&self, now: DateTime<Utc>) -> bool {
        self.paused_until.is_some_and(|until| now < until)
    }

    /// Whether the last look found the clipboard busy, so another should come soon.
    pub fn retry(&self) -> bool {
        self.retry
    }

    /// The size of a copy left out for being too large, once.
    pub fn take_too_big(&mut self) -> Option<usize> {
        self.too_big.take()
    }

    /// One look: a capture if something new, allowed and readable was copied.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Option<Capture> {
        let sequence = self.board.sequence();
        if sequence == self.last {
            return None;
        }
        if self.own.remove(&sequence) || self.paused(now) {
            self.last = sequence;
            return None;
        }
        if !self.board.begin() {
            // Another app has it open: this copy is read on the next look.
            self.retry = true;
            return None;
        }
        self.retry = false;
        self.last = sequence;
        let capture = if self.ignore_next {
            self.ignore_next = false;
            None
        } else {
            self.look()
        };
        self.board.end();
        capture
    }

    /// The names and markers, then (if they pass) the contents, with the clipboard open.
    fn look(&mut self) -> Option<Capture> {
        let names = self.board.formats();
        // A marker that is there but can't be read as allowing it counts as secret.
        let refused = |name: &str| {
            names.iter().any(|n| n == name) && !matches!(self.board.marker(name), Some(v) if v != 0)
        };
        let marks = Marks {
            no_history: refused(clip::CAN_INCLUDE_IN_HISTORY),
            no_cloud: refused(clip::CAN_UPLOAD_TO_CLOUD),
            names,
        };
        let app = (self.front)();
        if !allows(&marks, app.as_deref(), &self.ignored_apps) {
            return None;
        }
        self.read(&marks.names, app)
    }

    /// A format's bytes, unless it is over the size Clips keeps (then noted, to be said).
    fn bytes(&mut self, format: &str) -> Option<Vec<u8>> {
        if let Some(size) = self.board.size(format).filter(|s| *s > MAX_COPY) {
            self.too_big = Some(size);
            return None;
        }
        self.board.data(format).filter(|d| !d.is_empty())
    }

    /// The contents, read only now: files; a PNG; rich text with its plain text, or plain text; another picture.
    /// Text comes before a picture that isn't a PNG, as Excel's cells come with a picture Windows makes of them.
    fn read(&mut self, formats: &[String], source: Option<String>) -> Option<Capture> {
        let has = |f: &str| formats.iter().any(|n| n == f);
        let capture = |kind, text: String, data, data_type: Option<&str>, files| Capture {
            kind,
            text,
            data,
            data_type: data_type.map(str::to_string),
            files,
            source: source.clone(),
        };
        let image = |data| capture(Kind::Image, String::new(), Some(data), None, Vec::new());
        if has(clip::FILES) {
            let files = self.board.files();
            if !files.is_empty() {
                return Some(capture(Kind::Files, files.join("\n"), None, None, files));
            }
        }
        if has(clip::PNG)
            && let Some(data) = self.bytes(clip::PNG)
        {
            return Some(Capture {
                data_type: Some("png".into()),
                ..image(data)
            });
        }
        let text = if has(clip::TEXT) {
            if self.board.size(clip::TEXT).is_some_and(|s| s > MAX_COPY) {
                self.too_big = self.board.size(clip::TEXT);
                return None;
            }
            self.board.text().filter(|t| !t.is_empty())
        } else {
            None
        };
        if let Some(text) = text {
            for (format, kind) in [(clip::RTF, "rtf"), (clip::HTML, "html")] {
                if has(format)
                    && let Some(data) = self.bytes(format)
                {
                    return Some(capture(
                        Kind::Rich,
                        text,
                        Some(data),
                        Some(kind),
                        Vec::new(),
                    ));
                }
            }
            return Some(capture(Kind::Text, text, None, None, Vec::new()));
        }
        if has(clip::DIB)
            && let Some(data) = self.bytes(clip::DIB)
        {
            return Some(Capture {
                data_type: Some("dib".into()),
                ..image(data)
            });
        }
        None
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
    fn a_marker_that_cant_be_read_is_a_secret() {
        let mut w = watcher(FakeBoard::default(), "notepad.exe");
        w.board_mut().copy_text("hunter2");
        w.board_mut()
            .formats
            .push(clip::CAN_INCLUDE_IN_HISTORY.into());
        w.board_mut().unreadable_marker = true;
        assert_eq!(
            w.tick(Utc::now()),
            None,
            "an unreadable marker let a copy through"
        );
        // A marker saying 1 allows it.
        w.board_mut().copy_text("fine");
        w.board_mut()
            .formats
            .push(clip::CAN_INCLUDE_IN_HISTORY.into());
        w.board_mut().unreadable_marker = false;
        assert!(w.tick(Utc::now()).is_some());
    }

    #[test]
    fn a_busy_clipboard_is_looked_at_again() {
        let mut w = watcher(FakeBoard::default(), "notepad.exe");
        w.board_mut().copy_text("kept later");
        w.board_mut().locked = true;
        assert_eq!(w.tick(Utc::now()), None);
        assert!(w.retry(), "a busy look isn't retried");
        w.board_mut().locked = false;
        assert!(w.tick(Utc::now()).is_some(), "the copy was lost");
        assert!(!w.retry());
    }

    #[test]
    fn a_copy_too_large_is_declined_and_said() {
        let mut w = watcher(FakeBoard::default(), "notepad.exe");
        w.board_mut().copy_text("big");
        w.board_mut().declared_size = Some(MAX_COPY + 1);
        assert_eq!(w.tick(Utc::now()), None);
        assert_eq!(w.take_too_big(), Some(MAX_COPY + 1));
        assert_eq!(w.take_too_big(), None, "said once");
    }

    #[test]
    fn text_wins_over_a_picture_unless_its_a_png() {
        // Excel cells: text, rich text and a picture Windows made of them.
        let mut w = watcher(FakeBoard::default(), "excel.exe");
        let b = w.board_mut();
        b.sequence += 1;
        b.formats = vec![clip::TEXT.into(), clip::DIB.into(), clip::RTF.into()];
        b.text = Some("A1	B1".into());
        b.data = vec![
            (clip::DIB.into(), vec![1]),
            (clip::RTF.into(), b"{\rtf1 A1}".to_vec()),
        ];
        assert_eq!(w.tick(Utc::now()).map(|c| c.kind), Some(Kind::Rich));
        // A browser's image: a PNG, even with text beside it.
        let b = w.board_mut();
        b.sequence += 1;
        b.formats = vec![clip::PNG.into(), clip::DIB.into(), clip::TEXT.into()];
        b.text = Some("https://example.com/a.png".into());
        b.data = vec![
            (clip::PNG.into(), vec![137, 80]),
            (clip::DIB.into(), vec![1]),
        ];
        assert_eq!(
            w.tick(Utc::now()).map(|c| (c.kind, c.data_type)),
            Some((Kind::Image, Some("png".into())))
        );
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
