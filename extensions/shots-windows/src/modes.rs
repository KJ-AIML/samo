//! What the capture overlay is doing (ADR-004 S1, S4, S5): capturing an area or a window, choosing an area to scroll,
//! or reading text, picking a colour or measuring. Keys switch modes only before a drag, so a key pressed
//! mid-selection never changes what the selection is for. Esc always leaves. Ported from the Mac's `Modes.swift`.

pub use crate::clipboard::Clipboard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    Area,
    Window,
    Text,
    Colour,
    Ruler,
    Scroll,
}

impl OverlayMode {
    pub const ALL: [OverlayMode; 6] = [
        OverlayMode::Area,
        OverlayMode::Window,
        OverlayMode::Text,
        OverlayMode::Colour,
        OverlayMode::Ruler,
        OverlayMode::Scroll,
    ];

    /// A drag selects (area, text, scroll) or measures a box (ruler).
    pub fn drags(self) -> bool {
        matches!(
            self,
            OverlayMode::Area | OverlayMode::Text | OverlayMode::Ruler | OverlayMode::Scroll
        )
    }

    /// The overlay is dimmed around the selection. Never while picking a colour or measuring: they need true colours.
    pub fn dims(self) -> bool {
        matches!(
            self,
            OverlayMode::Area | OverlayMode::Window | OverlayMode::Text | OverlayMode::Scroll
        )
    }

    /// The magnifier follows the pointer.
    pub fn magnifies(self) -> bool {
        matches!(
            self,
            OverlayMode::Area | OverlayMode::Text | OverlayMode::Colour | OverlayMode::Scroll
        )
    }

    /// The words at the foot of the overlay.
    pub fn hint(self) -> &'static str {
        match self {
            OverlayMode::Area => {
                "Drag to capture  ·  Space for a window  ·  T text  ·  C colour  ·  R ruler  ·  S scroll  ·  Esc to cancel"
            }
            OverlayMode::Window => "Click a window  ·  Space for an area  ·  Esc to cancel",
            OverlayMode::Text => {
                "Drag over text to copy it  ·  T to capture instead  ·  Esc to cancel"
            }
            OverlayMode::Colour => {
                "Click to copy the hex  ·  Shift-click for rgb()  ·  C to capture instead  ·  Esc to cancel"
            }
            OverlayMode::Ruler => {
                "Hover to measure the gaps  ·  Drag for a box  ·  Click to copy  ·  R to capture instead  ·  Esc"
            }
            OverlayMode::Scroll => {
                "Drag over what you'll scroll  ·  then scroll it yourself  ·  S to capture instead  ·  Esc to cancel"
            }
        }
    }
}

/// What a key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyResult {
    Switched,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayModes {
    mode: OverlayMode,
}

impl OverlayModes {
    pub fn new(mode: OverlayMode) -> Self {
        Self { mode }
    }

    pub fn mode(&self) -> OverlayMode {
        self.mode
    }

    /// A key while the overlay is up: Space swaps area and window; T, C, R and S go to text, colour, ruler and
    /// scrolling, or back to capturing an area if already there. Nothing changes during a drag.
    pub fn key(&mut self, key: char, dragging: bool) -> KeyResult {
        if dragging {
            return KeyResult::Nothing;
        }
        let toggle =
            |to: OverlayMode, mode: OverlayMode| if mode == to { OverlayMode::Area } else { to };
        let next = match key.to_ascii_lowercase() {
            ' ' => match self.mode {
                OverlayMode::Area => OverlayMode::Window,
                OverlayMode::Window => OverlayMode::Area,
                _ => return KeyResult::Nothing,
            },
            't' => toggle(OverlayMode::Text, self.mode),
            'c' => toggle(OverlayMode::Colour, self.mode),
            'r' => toggle(OverlayMode::Ruler, self.mode),
            's' => toggle(OverlayMode::Scroll, self.mode),
            _ => return KeyResult::Nothing,
        };
        self.mode = next;
        KeyResult::Switched
    }
}

/// What reading a selection did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingOutcome {
    /// The text copied, if any.
    pub copied: Option<String>,
    /// QR codes found, for the card.
    pub codes: Vec<String>,
    pub note: String,
    pub problem: bool,
}

/// Reading a selection (T): its text (`lines`, as read) is copied, and any QR codes go to the card. Nothing is saved
/// or listed.
pub fn reading(
    lines: &[String],
    codes: &[String],
    clipboard: &mut impl Clipboard,
) -> ReadingOutcome {
    let outcome = |copied, note: &str, problem| ReadingOutcome {
        copied,
        codes: codes.to_vec(),
        note: note.to_string(),
        problem,
    };
    let Some(first) = lines.first() else {
        let note = if codes.is_empty() {
            "No text found"
        } else {
            "Found a QR code"
        };
        return outcome(None, note, false);
    };
    let text = lines.join("\n");
    if !clipboard.put_text(&text) {
        return outcome(None, "Couldn't copy the text", true);
    }
    let shown = if first.chars().count() > 48 {
        let mut cut: String = first.chars().take(47).collect();
        cut.push('…');
        cut
    } else {
        first.clone()
    };
    let more = match lines.len() {
        1 => String::new(),
        2 => " (+1 more line)".to_string(),
        n => format!(" (+{} more lines)", n - 1),
    };
    outcome(Some(text), &format!("Copied: {shown}{more}"), false)
}

/// How a scrolling capture ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollEnd {
    /// Esc, or a click outside the area: the person is done.
    Done,
    /// A frame didn't overlap the last: the page jumped.
    Lost,
    /// As tall as a shot goes.
    Full,
    /// The screen stopped coming.
    Failed,
}

impl ScrollEnd {
    /// What to say beside the usual note, if anything.
    pub fn note(self) -> Option<&'static str> {
        match self {
            ScrollEnd::Done => None,
            ScrollEnd::Lost => Some("The page jumped, so Shots kept what it had up to there"),
            ScrollEnd::Full => Some("That's as tall as a shot goes: Shots kept 30,000 px"),
            ScrollEnd::Failed => {
                Some("Windows stopped sending the screen, so Shots kept what it had")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clipboard that takes text, and fails when asked to.
    #[derive(Default)]
    struct TextBoard {
        text: Option<String>,
        broken: bool,
        images: u32,
    }

    impl Clipboard for TextBoard {
        fn put_png(&mut self, _png: &[u8]) -> bool {
            self.images += 1;
            false
        }
        fn put_text(&mut self, text: &str) -> bool {
            self.text = Some(text.to_string());
            !self.broken
        }
    }

    #[test]
    fn mode_keys_only_before_a_drag() {
        let mut modes = OverlayModes::new(OverlayMode::Area);
        assert_eq!(modes.key('t', false), KeyResult::Switched);
        assert_eq!(modes.mode(), OverlayMode::Text);
        // Mid-drag, a key changes nothing: the selection stays what it was for.
        assert_eq!(modes.key('c', true), KeyResult::Nothing);
        assert_eq!(modes.mode(), OverlayMode::Text);
        assert_eq!(modes.key('C', false), KeyResult::Switched);
        assert_eq!(modes.mode(), OverlayMode::Colour);
        // The same key again goes back to capturing.
        assert_eq!(modes.key('c', false), KeyResult::Switched);
        assert_eq!(modes.mode(), OverlayMode::Area);
        assert_eq!(modes.key('r', false), KeyResult::Switched);
        assert_eq!(modes.mode(), OverlayMode::Ruler);
        // Space swaps area and window only.
        assert_eq!(modes.key(' ', false), KeyResult::Nothing);
        let mut modes = OverlayModes::new(OverlayMode::Area);
        assert_eq!(modes.key(' ', false), KeyResult::Switched);
        assert_eq!(modes.mode(), OverlayMode::Window);
        assert_eq!(modes.key(' ', false), KeyResult::Switched);
        assert_eq!(modes.mode(), OverlayMode::Area);
        // S scrolls; pressed again, back to capturing. Other keys do nothing.
        assert_eq!(modes.key('s', false), KeyResult::Switched);
        assert_eq!(modes.mode(), OverlayMode::Scroll);
        assert_eq!(modes.key('s', true), KeyResult::Nothing, "not mid-drag");
        assert_eq!(modes.key('s', false), KeyResult::Switched);
        assert_eq!(modes.key('q', false), KeyResult::Nothing);
        assert_eq!(modes.mode(), OverlayMode::Area);
    }

    #[test]
    fn scroll_mode_key() {
        assert!(OverlayMode::Area.hint().contains("S scroll"));
        assert!(OverlayMode::Scroll.hint().contains("scroll it yourself"));
        assert!(OverlayMode::Scroll.drags() && OverlayMode::Scroll.dims());
        // How a scroll ended says what happened only when it wasn't the person ending it.
        assert_eq!(ScrollEnd::Done.note(), None);
        assert!(ScrollEnd::Lost.note().is_some_and(|n| n.contains("jumped")));
        assert!(ScrollEnd::Full.note().is_some_and(|n| n.contains("30,000")));
        assert!(
            ScrollEnd::Failed
                .note()
                .is_some_and(|n| n.contains("Windows"))
        );
    }

    #[test]
    fn the_hint_names_the_mode() {
        assert!(OverlayMode::Area.hint().contains("T text"));
        assert!(OverlayMode::Text.hint().starts_with("Drag over text"));
        assert!(OverlayMode::Colour.hint().contains("hex"));
        assert!(OverlayMode::Colour.hint().contains("Shift-click"));
        assert!(OverlayMode::Ruler.hint().starts_with("Hover to measure"));
        for mode in OverlayMode::ALL {
            assert!(mode.hint().contains("Esc"), "{mode:?} says how to leave");
        }
        // Colours and measuring need the screen's true colours.
        assert!(!OverlayMode::Colour.dims() && !OverlayMode::Ruler.dims());
        assert!(OverlayMode::Text.dims());
    }

    #[test]
    fn reading_copies_text_and_nothing_else() {
        let mut board = TextBoard::default();
        let out = reading(&["สวัสดี".into(), "two".into()], &[], &mut board);
        assert_eq!(board.text.as_deref(), Some("สวัสดี\ntwo"));
        assert_eq!(out.copied.as_deref(), Some("สวัสดี\ntwo"));
        assert_eq!(out.note, "Copied: สวัสดี (+1 more line)");
        assert!(!out.problem);
        assert_eq!(board.images, 0, "reading never copies an image");
    }

    #[test]
    fn reading_says_what_it_found() {
        let mut board = TextBoard::default();
        assert_eq!(reading(&[], &[], &mut board).note, "No text found");
        assert_eq!(board.text, None, "nothing found, nothing copied");
        let code = reading(&[], &["https://x.com".into()], &mut board);
        assert_eq!(code.note, "Found a QR code");
        assert_eq!(code.codes, ["https://x.com"]);
        let long = reading(&["ก".repeat(80)], &[], &mut board);
        assert_eq!(long.note.chars().count(), "Copied: ".len() + 48);
        board.broken = true;
        let failed = reading(&["a".into()], &[], &mut board);
        assert!(failed.problem);
        assert_eq!(failed.copied, None);
    }
}
