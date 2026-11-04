//! Scrolling capture's stitching (ADR-004 S5): each new frame of the area is matched against the last frame used, and
//! the rows the scroll revealed are added below. Rows are compared by fingerprints (a few column sums each), only
//! rows with detail count, so blank margins can't fake a match, and rows that stay put at the top and bottom (a sticky
//! header or footer) are kept once. Strips are copied out of their frames, so a frame is let go once matched (R17).
//! Ported from the Mac's `Stitcher.swift`.

use image::{RgbaImage, imageops};

/// As tall as a shot goes.
pub const MAX_HEIGHT: usize = 30_000;
/// As large as a stitched shot goes in memory.
pub const MAX_BYTES: usize = 400 * 1024 * 1024;
const BUCKETS: usize = 32;

/// One frame of the area: its picture and a fingerprint per row.
pub struct ScrollFrame {
    image: RgbaImage,
    width: usize,
    height: usize,
    /// Per row, the sum of red, green and blue in each of the buckets' columns.
    prints: Vec<Vec<i64>>,
    /// Per row, the sum of its prints.
    sums: Vec<i64>,
    /// Per row: has detail (its columns differ), so it can tell one place from another.
    detailed: Vec<bool>,
    bucket_width: usize,
}

impl ScrollFrame {
    /// The frame, if it is at least 8 × 8.
    pub fn new(image: RgbaImage) -> Option<Self> {
        let (w, h) = (image.width() as usize, image.height() as usize);
        if w < 8 || h < 8 {
            return None;
        }
        let buckets = BUCKETS.min(w);
        let bucket_width = w / buckets;
        let raw = image.as_raw();
        let mut prints = Vec::with_capacity(h);
        let mut sums = Vec::with_capacity(h);
        let mut detailed = Vec::with_capacity(h);
        for row in raw.chunks_exact(w * 4).take(h) {
            let mut print = vec![0i64; buckets];
            for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
                let bucket = (x / bucket_width).min(buckets - 1);
                if let (Some(slot), [r, g, b, _]) = (print.get_mut(bucket), px) {
                    *slot += i64::from(*r) + i64::from(*g) + i64::from(*b);
                }
            }
            let low = print.iter().copied().min().unwrap_or(0);
            let high = print.iter().copied().max().unwrap_or(0);
            sums.push(print.iter().sum());
            // Detail: the columns differ by more than a few levels a pixel.
            detailed.push(high - low > (bucket_width * 18) as i64);
            prints.push(print);
        }
        Some(Self {
            image,
            width: w,
            height: h,
            prints,
            sums,
            detailed,
            bucket_width,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    fn sum(&self, row: usize) -> i64 {
        self.sums.get(row).copied().unwrap_or(0)
    }

    fn detailed(&self, row: usize) -> bool {
        self.detailed.get(row).copied().unwrap_or(false)
    }

    /// Whether row `a` here and row `b` of `other` look the same.
    fn same(&self, a: usize, other: &ScrollFrame, b: usize) -> bool {
        if (self.sum(a) - other.sum(b)).abs() > (self.width * 3) as i64 {
            return false;
        }
        let tolerance = (self.bucket_width * 6) as i64;
        match (self.prints.get(a), other.prints.get(b)) {
            (Some(x), Some(y)) => x.iter().zip(y).all(|(p, q)| (p - q).abs() <= tolerance),
            _ => false,
        }
    }
}

/// What adding a frame did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// This many new rows were added below.
    Added(usize),
    /// Nothing moved.
    Unchanged,
    /// The page went back up: ignored.
    ScrolledBack,
    /// No overlap with the last frame (the page jumped): stop, keeping what came before.
    Lost,
    /// The cap was reached: stop.
    Full,
}

pub struct Stitcher {
    width: usize,
    max_height: usize,
    max_bytes: usize,
    reference: ScrollFrame,
    strips: Vec<RgbaImage>,
    footer: Option<RgbaImage>,
    finished: bool,
}

/// Rows `from..to` of an image, copied into their own memory, so the frame can go.
fn copy(image: &RgbaImage, from: usize, to: usize) -> Option<RgbaImage> {
    (to > from).then(|| {
        imageops::crop_imm(image, 0, from as u32, image.width(), (to - from) as u32).to_image()
    })
}

impl Stitcher {
    pub fn new(first: ScrollFrame) -> Self {
        Self::with_caps(first, MAX_HEIGHT, MAX_BYTES)
    }

    pub fn with_caps(first: ScrollFrame, max_height: usize, max_bytes: usize) -> Self {
        Self {
            width: first.width,
            max_height,
            max_bytes,
            reference: first,
            strips: Vec::new(),
            footer: None,
            finished: false,
        }
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    /// The height of the picture so far.
    pub fn height(&self) -> usize {
        if self.strips.is_empty() {
            return self.reference.height;
        }
        let body: usize = self.strips.iter().map(|s| s.height() as usize).sum();
        body + self.footer.as_ref().map_or(0, |f| f.height() as usize)
    }

    /// The picture so far, as strips top to bottom (for the preview).
    pub fn parts(&self) -> Vec<&RgbaImage> {
        if self.strips.is_empty() {
            return vec![&self.reference.image];
        }
        self.strips.iter().chain(self.footer.as_ref()).collect()
    }

    pub fn add(&mut self, frame: ScrollFrame) -> Step {
        if self.finished {
            return Step::Full;
        }
        if frame.width != self.width || frame.height != self.reference.height {
            return Step::Lost;
        }
        let h = frame.height;
        // Rows that stayed put at the top and bottom: a sticky header and footer, or nothing moved at all.
        let mut header = 0;
        while header < h && frame.same(header, &self.reference, header) {
            header += 1;
        }
        if header == h {
            return Step::Unchanged;
        }
        let mut footer = 0;
        while footer < h - header && frame.same(h - 1 - footer, &self.reference, h - 1 - footer) {
            footer += 1;
        }
        let header = header.min(h / 3);
        let footer = footer.min(h / 3);
        let top = header;
        let middle = h - header - footer;
        let min_overlap = 24.max(middle / 6);
        if middle <= min_overlap {
            return Step::Unchanged;
        }
        // Mostly the same where it was: something small changed (a caret, a spinner), not a scroll.
        if self.score(&frame, 0, top, middle).is_none_or(|s| s >= 0.9) {
            return Step::Unchanged;
        }
        let shifts = 1..=(middle - min_overlap) as i64;
        if let Some(d) = shifts
            .clone()
            .find(|d| self.matches(&frame, *d, top, middle))
        {
            return self.append(frame, d as usize, footer);
        }
        if shifts
            .into_iter()
            .any(|d| self.matches(&frame, -d, top, middle))
        {
            return Step::ScrolledBack;
        }
        Step::Lost
    }

    /// The rows compared when the new frame's content sits `shift` rows higher (a scroll down) than the reference's.
    fn pairs(shift: i64, top: usize, middle: usize) -> impl Iterator<Item = (usize, usize)> {
        let overlap = middle.saturating_sub(shift.unsigned_abs() as usize);
        let s = shift.unsigned_abs() as usize;
        (0..overlap).map(move |i| {
            if shift >= 0 {
                (top + i, top + i + s)
            } else {
                (top + i + s, top + i)
            }
        })
    }

    /// The share of detailed rows that match at `shift`; `None` if too few detailed rows overlap to tell.
    fn score(&self, frame: &ScrollFrame, shift: i64, top: usize, middle: usize) -> Option<f64> {
        let (mut tested, mut matched) = (0usize, 0usize);
        for (new, old) in Self::pairs(shift, top, middle) {
            if !frame.detailed(new) {
                continue;
            }
            tested += 1;
            if frame.same(new, &self.reference, old) {
                matched += 1;
            }
        }
        (tested >= 8).then(|| matched as f64 / tested as f64)
    }

    /// Whether `shift` lines up: quick on sums with an early out, then checked row by row.
    fn matches(&self, frame: &ScrollFrame, shift: i64, top: usize, middle: usize) -> bool {
        let (mut tested, mut missed) = (0usize, 0usize);
        for (new, old) in Self::pairs(shift, top, middle) {
            if !frame.detailed(new) {
                continue;
            }
            tested += 1;
            if (frame.sum(new) - self.reference.sum(old)).abs() > (frame.width * 3) as i64 {
                missed += 1;
                if missed > 2 + tested / 40 {
                    return false;
                }
            }
        }
        if tested < 8 || missed > tested / 40 {
            return false;
        }
        self.score(frame, shift, top, middle).unwrap_or(0.0) >= 0.97
    }

    fn append(&mut self, frame: ScrollFrame, d: usize, footer: usize) -> Step {
        let h = frame.height;
        if self.strips.is_empty() {
            // The first move: the first frame down to its footer.
            let Some(base) = copy(&self.reference.image, 0, h - footer) else {
                return Step::Lost;
            };
            self.strips.push(base);
        }
        let mut rows = d;
        let mut step = Step::Added(d);
        // Room below what is kept so far; at the cap the footer is left off, so it doesn't count.
        let body: usize = self.strips.iter().map(|s| s.height() as usize).sum();
        let room = self
            .max_height
            .min(self.max_bytes / (self.width * 4))
            .saturating_sub(body);
        if rows >= room {
            rows = room;
            step = Step::Full;
            self.finished = true;
        }
        if let Some(strip) = copy(&frame.image, h - footer - d, h - footer - d + rows) {
            self.strips.push(strip);
        }
        self.footer = if footer > 0 && !self.finished {
            copy(&frame.image, h - footer, h)
        } else {
            None
        };
        self.reference = frame;
        step
    }

    /// The whole picture, top to bottom.
    pub fn image(&self) -> RgbaImage {
        let parts = self.parts();
        let total: u32 = parts.iter().map(|p| p.height()).sum();
        let mut out = RgbaImage::new(self.width as u32, total);
        let mut y = 0i64;
        for part in parts {
            imageops::replace(&mut out, part, 0, y);
            y += i64::from(part.height());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A synthetic web page, rows from the top: bands of colour, sparse dark dots unique to every row (like text), and
    /// a blank gap in the middle.
    struct Page {
        width: u32,
        height: u32,
        header: u32,
        footer: u32,
        view: u32,
    }

    type Rgb = (u8, u8, u8);

    impl Page {
        fn new(height: u32) -> Self {
            Self {
                width: 400,
                height,
                header: 50,
                footer: 30,
                view: 520,
            }
        }

        /// The page's own pixel at `x`, `y` (rows from the top).
        fn pixel(&self, x: u32, y: u32) -> Rgb {
            if (900..960).contains(&y) {
                return (255, 255, 255); // a blank gap
            }
            let mut h = (i64::from(x).wrapping_mul(73_856_093)
                ^ i64::from(y).wrapping_mul(19_349_663)) as u32;
            h ^= h >> 13;
            h = h.wrapping_mul(0x5bd1_e995);
            h ^= h >> 15;
            if h.is_multiple_of(29) {
                return (20, 20, 30);
            }
            let band = ((y / 40) % 6) as u8;
            (230 - band * 20, 235, 240 - band * 9)
        }

        /// The header (dark, with a light logo) and footer (mid grey with a button), the same in every frame.
        fn header_pixel(&self, x: u32, y: u32) -> Rgb {
            if x > 20 && x < 120 && y > 15 && y < 35 {
                (240, 240, 240)
            } else {
                (36, 42, 51)
            }
        }

        fn footer_pixel(&self, x: u32, y: u32) -> Rgb {
            if x > 280 && x < 380 && y > 6 && y < 24 {
                (36, 62, 181)
            } else {
                (200, 204, 210)
            }
        }

        fn image(&self, rows: u32, colour: impl Fn(u32, u32) -> Rgb) -> RgbaImage {
            RgbaImage::from_fn(self.width, rows, |x, y| {
                let (r, g, b) = colour(x, y);
                Rgba([r, g, b, 255])
            })
        }

        /// What the area shows with the page scrolled to `offset`: the sticky header, the page, the sticky footer.
        fn frame(&self, offset: u32, sticky: bool) -> Option<ScrollFrame> {
            let rows = if sticky {
                self.header + self.view + self.footer
            } else {
                self.view
            };
            ScrollFrame::new(self.image(rows, |x, y| {
                if !sticky {
                    return self.pixel(x, offset + y);
                }
                if y < self.header {
                    self.header_pixel(x, y)
                } else if y >= self.header + self.view {
                    self.footer_pixel(x, y - self.header - self.view)
                } else {
                    self.pixel(x, offset + y - self.header)
                }
            }))
        }

        /// The whole page as one capture should come out: the header once, every row once, the footer once.
        fn expected(&self) -> RgbaImage {
            self.image(self.header + self.height + self.footer, |x, y| {
                if y < self.header {
                    self.header_pixel(x, y)
                } else if y >= self.header + self.height {
                    self.footer_pixel(x, y - self.header - self.height)
                } else {
                    self.pixel(x, y - self.header)
                }
            })
        }
    }

    #[test]
    fn a_page_three_screens_long_stitches_exactly() -> Result<(), String> {
        // Three screens and a bit: 1,880 rows of page under a 520-row view.
        let page = Page::new(1880);
        let mut stitcher = Stitcher::new(page.frame(0, true).ok_or("frame")?);
        // Uneven scrolls by hand: a pause, a tiny move, a scroll back, big moves, and to the very end.
        let mut steps = Vec::new();
        for offset in [0, 23, 24, 140, 140, 310, 290, 520, 760, 1000, 1180, 1360] {
            steps.push(stitcher.add(page.frame(offset, true).ok_or("frame")?));
        }
        assert_eq!(
            steps,
            [
                Step::Unchanged,
                Step::Added(23),
                Step::Added(1),
                Step::Added(116),
                Step::Unchanged,
                Step::Added(170),
                Step::ScrolledBack,
                Step::Added(210),
                Step::Added(240),
                Step::Added(240),
                Step::Added(180),
                Step::Added(180)
            ]
        );
        // No seams, no repeats: the header once, the page row for row, the footer once.
        let image = stitcher.image();
        assert_eq!(image.height(), page.header + page.height + page.footer);
        assert!(
            image == page.expected(),
            "the stitched page differs from the page"
        );
        Ok(())
    }

    #[test]
    fn without_sticky_bands_too() -> Result<(), String> {
        let page = Page::new(1400);
        let mut stitcher = Stitcher::new(page.frame(0, false).ok_or("frame")?);
        for offset in [100, 330, 600, 880] {
            assert_ne!(
                stitcher.add(page.frame(offset, false).ok_or("frame")?),
                Step::Lost,
                "{offset}"
            );
        }
        let image = stitcher.image();
        assert_eq!(image.height(), 880 + page.view);
        assert!(image == page.image(880 + page.view, |x, y| page.pixel(x, y)));
        Ok(())
    }

    #[test]
    fn a_jump_stops_and_keeps_what_it_has() -> Result<(), String> {
        let page = Page::new(1880);
        let mut stitcher = Stitcher::new(page.frame(0, true).ok_or("frame")?);
        assert_eq!(
            stitcher.add(page.frame(200, true).ok_or("frame")?),
            Step::Added(200)
        );
        // The page jumped far below: nothing overlaps.
        assert_eq!(
            stitcher.add(page.frame(1300, true).ok_or("frame")?),
            Step::Lost
        );
        assert_eq!(
            stitcher.image().height(),
            page.header + 200 + page.view + page.footer
        );
        Ok(())
    }

    #[test]
    fn blank_pages_dont_repeat() -> Result<(), String> {
        let page = Page::new(1880);
        let blank = || ScrollFrame::new(page.image(600, |_, _| (255, 255, 255)));
        let mut stitcher = Stitcher::new(blank().ok_or("frame")?);
        // Scrolling a blank page shows the same frame: nothing to add, nothing repeated.
        assert_eq!(stitcher.add(blank().ok_or("frame")?), Step::Unchanged);
        assert_eq!(stitcher.height(), 600);
        // The blank gap scrolled past keeps its place in the page, matched by the rows with detail.
        let mut gap = Stitcher::new(page.frame(700, true).ok_or("frame")?);
        assert_eq!(
            gap.add(page.frame(760, true).ok_or("frame")?),
            Step::Added(60)
        );
        Ok(())
    }

    #[test]
    fn the_cap_stops_it() -> Result<(), String> {
        let page = Page::new(1880);
        let mut stitcher = Stitcher::with_caps(page.frame(0, true).ok_or("frame")?, 900, MAX_BYTES);
        assert_eq!(
            stitcher.add(page.frame(200, true).ok_or("frame")?),
            Step::Added(200)
        );
        assert_eq!(
            stitcher.add(page.frame(400, true).ok_or("frame")?),
            Step::Full
        );
        assert!(stitcher.finished());
        assert_eq!(
            stitcher.add(page.frame(600, true).ok_or("frame")?),
            Step::Full
        );
        assert_eq!(stitcher.image().height(), 900);
        // Frames of another size never fit.
        let mut other = Stitcher::new(page.frame(0, true).ok_or("frame")?);
        assert_eq!(other.add(page.frame(10, false).ok_or("frame")?), Step::Lost);
        Ok(())
    }

    #[test]
    fn stitching_is_quick_enough() -> Result<(), String> {
        // A 1,600 × 1,200 area: fingerprinting and matching a frame fit in a frame's time at 20 a second.
        let page = Page {
            width: 1600,
            height: 3000,
            header: 50,
            footer: 30,
            view: 1120,
        };
        let first = page.frame(0, true).ok_or("frame")?;
        let image = page.image(1200, |x, y| {
            if y < page.header {
                page.header_pixel(x, y)
            } else if y >= page.header + page.view {
                page.footer_pixel(x, y - page.header - page.view)
            } else {
                page.pixel(x, 90 + y - page.header)
            }
        });
        let mut stitcher = Stitcher::new(first);
        let start = std::time::Instant::now();
        let next = ScrollFrame::new(image).ok_or("frame")?;
        assert_eq!(stitcher.add(next), Step::Added(90));
        let took = start.elapsed();
        // Unoptimised test builds are far slower; the helper Samo runs is a release build.
        let budget = if cfg!(debug_assertions) { 2.0 } else { 0.05 };
        assert!(took.as_secs_f64() < budget, "{took:?}");
        Ok(())
    }
}
