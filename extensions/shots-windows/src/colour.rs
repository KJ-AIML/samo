//! Colours and pixels for the colour picker and the ruler (ADR-004 S4): a shot's pixels, rows from the top. Windows'
//! screen pixels aren't colour-managed, so a captured pixel is the colour the app drew. Ported from the Mac's
//! `Colour.swift` (which converts from the display's space to sRGB).

use image::RgbaImage;

/// One pixel's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Pixel {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `#F15B2A`.
    pub fn hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// `rgb(241, 91, 42)`.
    pub fn css(&self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// Whether `other` is within `tolerance` of this on every channel.
    pub fn near(&self, other: &Pixel, tolerance: u8) -> bool {
        self.r.abs_diff(other.r) <= tolerance
            && self.g.abs_diff(other.g) <= tolerance
            && self.b.abs_diff(other.b) <= tolerance
    }
}

/// A shot's pixels, rows from the top. Made when the colour picker or the ruler first needs it, and let go with the
/// window that made it (R17).
pub struct PixelGrid<'a> {
    image: &'a RgbaImage,
}

impl<'a> PixelGrid<'a> {
    pub fn new(image: &'a RgbaImage) -> Option<Self> {
        (image.width() > 0 && image.height() > 0).then_some(Self { image })
    }

    pub fn width(&self) -> i64 {
        i64::from(self.image.width())
    }

    pub fn height(&self) -> i64 {
        i64::from(self.image.height())
    }

    /// The pixel at `x`, `y` (rows from the top), or `None` outside.
    pub fn pixel(&self, x: i64, y: i64) -> Option<Pixel> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        let p = self.image.get_pixel_checked(x, y)?;
        Some(Pixel::new(p[0], p[1], p[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_as_hex_and_css() {
        assert_eq!(Pixel::new(0xF1, 0x5B, 0x2A).hex(), "#F15B2A");
        assert_eq!(Pixel::new(241, 91, 42).css(), "rgb(241, 91, 42)");
        assert_eq!(Pixel::new(0, 10, 255).hex(), "#000AFF");
        let image = RgbaImage::from_pixel(4, 4, image::Rgba([0xF1, 0x5B, 0x2A, 255]));
        let grid = PixelGrid::new(&image);
        assert_eq!(
            grid.and_then(|g| g.pixel(1, 1)),
            Some(Pixel::new(0xF1, 0x5B, 0x2A))
        );
        assert!(Pixel::new(10, 10, 10).near(&Pixel::new(11, 9, 10), 1));
        assert!(!Pixel::new(10, 10, 10).near(&Pixel::new(12, 10, 10), 1));
    }
}
