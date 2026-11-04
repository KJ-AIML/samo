//! The ruler (ADR-004 S4): from the pixel under the pointer, how far the same colour runs left, right, up and down.
//! An edge is the first pixel whose colour differs by more than a tolerance on any channel, so an anti-aliased edge
//! still stops the ruler, and a screenshot's faint noise doesn't. Ported from the Mac's `Edges.swift`.

use crate::colour::PixelGrid;

/// Out of 255, on any channel.
pub const TOLERANCE: u8 = 16;

/// How far the pointer's colour runs each way, in pixels, not counting the pointer's own pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub left: i64,
    pub right: i64,
    pub up: i64,
    pub down: i64,
}

impl Span {
    /// The width of the run across the pointer: from one edge to the other.
    pub fn width(&self) -> i64 {
        self.left + self.right + 1
    }

    pub fn height(&self) -> i64 {
        self.up + self.down + 1
    }
}

/// The span around `x`, `y`; `None` outside the image.
pub fn around(grid: &PixelGrid, x: i64, y: i64, tolerance: u8) -> Option<Span> {
    let start = grid.pixel(x, y)?;
    let run = |dx: i64, dy: i64| {
        let mut n = 0;
        let (mut cx, mut cy) = (x + dx, y + dy);
        while grid
            .pixel(cx, cy)
            .is_some_and(|p| p.near(&start, tolerance))
        {
            n += 1;
            cx += dx;
            cy += dy;
        }
        n
    };
    Some(Span {
        left: run(-1, 0),
        right: run(1, 0),
        up: run(0, -1),
        down: run(0, 1),
    })
}

/// Pixels as CSS pixels (what a page at 100 % uses): whole when they are, else to a tenth.
fn css(pixels: i64, scale: f64) -> String {
    let value = pixels as f64 / scale;
    if value == value.round() {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// `32 px`, or on a scaled monitor `48 px · 32 CSS px`.
pub fn label(pixels: i64, scale: f64) -> String {
    if scale <= 1.0 {
        return format!("{pixels} px");
    }
    format!("{pixels} px · {} CSS px", css(pixels, scale))
}

/// `120 × 40 px`, or on a scaled monitor `120 × 40 px · 60 × 20 CSS px`.
pub fn size(width: i64, height: i64, scale: f64) -> String {
    if scale <= 1.0 {
        return format!("{width} × {height} px");
    }
    format!(
        "{width} × {height} px · {} × {} CSS px",
        css(width, scale),
        css(height, scale)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use image::{Rgba, RgbaImage};

    fn fill(image: &mut RgbaImage, r: Rect, colour: [u8; 3]) {
        let (x0, y0) = (r.x as u32, r.y as u32);
        for y in y0..y0 + r.h as u32 {
            for x in x0..x0 + r.w as u32 {
                image.put_pixel(x, y, Rgba([colour[0], colour[1], colour[2], 255]));
            }
        }
    }

    fn grow(r: Rect, by: f64) -> Rect {
        Rect::new(r.x - by, r.y - by, r.w + 2.0 * by, r.h + 2.0 * by)
    }

    /// A button on a white page (rows from the top), its label block inside, and optionally a soft (anti-aliased)
    /// one-pixel edge around both and faint noise inside.
    fn button_page(button: Rect, label: Rect, soft: bool) -> RgbaImage {
        let mut page = RgbaImage::from_pixel(300, 200, Rgba([255, 255, 255, 255]));
        if soft {
            fill(&mut page, grow(button, 1.0), [145, 158, 217]);
        }
        fill(&mut page, button, [0x24, 0x3E, 0xB5]);
        if soft {
            fill(&mut page, grow(label, 1.0), [18, 31, 89]);
            // Faint noise in the button: within the tolerance.
            let mut x = button.x as u32 + 3;
            while x < (button.x + button.w) as u32 - 3 {
                page.put_pixel(x, button.y as u32 + 3, Rgba([0x27, 0x3B, 0xB8, 255]));
                x += 7;
            }
        }
        fill(&mut page, label, [0, 0, 0]);
        page
    }

    const BUTTON: Rect = Rect::new(50.0, 60.0, 200.0, 80.0);
    const LABEL: Rect = Rect::new(82.0, 84.0, 136.0, 32.0);

    #[test]
    fn padding_measures_exactly() -> Result<(), String> {
        let page = button_page(BUTTON, LABEL, false);
        let grid = PixelGrid::new(&page).ok_or("no grid")?;
        // In the left padding: from the button's edge to the label's.
        let left = around(&grid, 60, 100, TOLERANCE).ok_or("no span")?;
        assert_eq!(left.width(), 32);
        assert_eq!(left.height(), 80, "nothing else in that column");
        // Above the label: the top padding.
        let above = around(&grid, 150, 70, TOLERANCE).ok_or("no span")?;
        assert_eq!(
            (above.width(), above.height(), above.up, above.down),
            (200, 24, 10, 13)
        );
        // On the label itself: its size.
        assert_eq!(
            around(&grid, 100, 90, TOLERANCE).map(|s| (s.width(), s.height())),
            Some((136, 32))
        );
        Ok(())
    }

    #[test]
    fn soft_edges_are_one_edge() -> Result<(), String> {
        let page = button_page(BUTTON, LABEL, true);
        let grid = PixelGrid::new(&page).ok_or("no grid")?;
        // The blended pixels stop the ruler; the faint noise inside doesn't.
        let left = around(&grid, 60, 100, TOLERANCE).ok_or("no span")?;
        assert_eq!(
            left.width(),
            31,
            "the label's blended edge is its own pixel"
        );
        let above = around(&grid, 150, 70, TOLERANCE).ok_or("no span")?;
        assert_eq!(
            above.width(),
            200,
            "noise within the tolerance is the same colour"
        );
        assert_eq!(above.height(), 23);
        Ok(())
    }

    #[test]
    fn edges_stop_at_the_image() -> Result<(), String> {
        let page = RgbaImage::from_pixel(120, 80, Rgba([51, 77, 179, 255]));
        let grid = PixelGrid::new(&page).ok_or("no grid")?;
        assert_eq!(
            around(&grid, 10, 10, TOLERANCE),
            Some(Span {
                left: 10,
                right: 109,
                up: 10,
                down: 69
            })
        );
        assert_eq!(around(&grid, 120, 0, TOLERANCE), None);
        Ok(())
    }

    #[test]
    fn ruler_labels_in_pixels_and_css_pixels() {
        assert_eq!(label(32, 2.0), "32 px · 16 CSS px");
        assert_eq!(label(33, 2.0), "33 px · 16.5 CSS px");
        assert_eq!(label(48, 1.5), "48 px · 32 CSS px");
        assert_eq!(label(32, 1.0), "32 px");
        assert_eq!(size(120, 40, 2.0), "120 × 40 px · 60 × 20 CSS px");
    }
}
