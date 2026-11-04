//! The clipboard as Shots uses it: a shot as an image, or text read from one (or a colour, or a size). Tests stand in
//! for Windows'. Ported from the Mac's `Clipboard` protocol (`AfterCapture.swift`).

pub trait Clipboard {
    /// A shot, as PNG (Windows' gets a DIB beside it).
    fn put_png(&mut self, png: &[u8]) -> bool;
    /// Text read from a shot, or a colour, or a size.
    fn put_text(&mut self, text: &str) -> bool;
}
