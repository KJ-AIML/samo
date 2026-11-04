//! A PNG as a DIB (`CF_DIB`): an image Clips kept as PNG goes back with both, since many Windows apps read only DIBs
//! and Windows doesn't make one from a PNG. Pure, so tested on every OS.

use std::io::Cursor;

/// `BITMAPINFOHEADER`'s size.
const HEADER: usize = 40;

/// The PNG as a 32-bit bottom-up DIB (BGRA, `BI_RGB`); `None` if it can't be read, or its pixels would be more than
/// `max_bytes`.
pub fn from_png(png: &[u8], max_bytes: usize) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(png),
        png::Limits {
            bytes: max_bytes.saturating_mul(2),
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut pixels).ok()?;
    let width = usize::try_from(frame.width).ok()?;
    let height = usize::try_from(frame.height).ok()?;
    let size = width.checked_mul(height)?.checked_mul(4)?;
    if size > max_bytes || width == 0 || height == 0 {
        return None;
    }
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let mut dib = Vec::with_capacity(HEADER + size);
    for value in [u32::try_from(HEADER).ok()?, frame.width, frame.height] {
        dib.extend_from_slice(&value.to_le_bytes());
    }
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    for value in [0, u32::try_from(size).ok()?, 0, 0, 0, 0] {
        // BI_RGB, the image's size, resolution, colours used and important
        dib.extend_from_slice(&value.to_le_bytes());
    }
    // Rows from the bottom up.
    for row in pixels.chunks(frame.line_size).take(height).rev() {
        for px in row.chunks(channels).take(width) {
            let (r, g, b, a) = match *px {
                [v] => (v, v, v, 255),
                [v, a] => (v, v, v, a),
                [r, g, b] => (r, g, b, 255),
                [r, g, b, a] => (r, g, b, a),
                _ => return None,
            };
            dib.extend_from_slice(&[b, g, r, a]);
        }
    }
    (dib.len() == HEADER + size).then_some(dib)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG of `pixels` (RGBA, row by row from the top).
    pub(crate) fn png_of(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        if let Ok(mut writer) = encoder.write_header() {
            let _ = writer.write_image_data(pixels);
        }
        out
    }

    fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
        bytes
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    }

    #[test]
    fn a_png_becomes_a_bottom_up_bgra_dib() {
        // Red on top, half-clear blue below.
        let png = png_of(1, 2, &[255, 0, 0, 255, 0, 0, 255, 128]);
        let dib = from_png(&png, 1 << 20).unwrap_or_default();
        assert_eq!(u32_at(&dib, 0), Some(40), "header size");
        assert_eq!(u32_at(&dib, 4), Some(1), "width");
        assert_eq!(u32_at(&dib, 8), Some(2), "height (bottom-up)");
        assert_eq!(dib.get(12..16), Some(&[1, 0, 32, 0][..]), "planes, 32 bits");
        assert_eq!(u32_at(&dib, 16), Some(0), "BI_RGB");
        assert_eq!(u32_at(&dib, 20), Some(8), "image size");
        // The bottom row first: blue, then red; BGRA.
        assert_eq!(dib.get(40..48), Some(&[255, 0, 0, 128, 0, 0, 255, 255][..]));
        assert_eq!(dib.len(), 48);
    }

    #[test]
    fn grey_and_palette_pngs_become_bgra_too() {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        if let Ok(mut writer) = encoder.write_header() {
            let _ = writer.write_image_data(&[0, 200]);
        }
        let dib = from_png(&out, 1 << 20).unwrap_or_default();
        assert_eq!(
            dib.get(40..48),
            Some(&[0, 0, 0, 255, 200, 200, 200, 255][..])
        );
    }

    #[test]
    fn a_damaged_or_huge_png_gives_nothing() {
        assert_eq!(from_png(b"not a png", 1 << 20), None);
        let png = png_of(4, 4, &[0; 64]);
        assert_eq!(from_png(&png, 32), None, "4x4x4 = 64 bytes is over 32");
    }
}
