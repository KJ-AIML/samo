//! Turning a shot into a file's bytes and back, and full resolution into one-to-one. A shot's density says its scale
//! (pixels per Windows pixel at 100 %: 96 dpi is 1, 144 dpi is 1.5), so Shots opens it at its real size. Ported from
//! the Mac's `Encoding.swift` (which counts from 72 dpi).

use crate::settings::Format;
use image::buffer::ConvertBuffer;
use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use image::{ImageReader, RgbaImage, imageops};
use std::io::Cursor;

/// Windows' pixels per inch at 100 %.
const BASE_DPI: f64 = 96.0;
const INCHES_PER_METRE: f64 = 39.370_08;

/// `image` as PNG or JPEG (quality 0.9), with `scale` written as its density.
pub fn data(image: &RgbaImage, format: Format, scale: f64) -> Option<Vec<u8>> {
    let dpi = BASE_DPI * scale.max(1.0);
    let mut out = Vec::new();
    match format {
        Format::Png => {
            let mut encoder = png::Encoder::new(&mut out, image.width(), image.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let per_metre = (dpi * INCHES_PER_METRE).round() as u32;
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: per_metre,
                yppu: per_metre,
                unit: png::Unit::Meter,
            }));
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(image.as_raw()).ok()?;
            writer.finish().ok()?;
        }
        Format::Jpeg => {
            let mut encoder = JpegEncoder::new_with_quality(&mut out, 90);
            encoder.set_pixel_density(PixelDensity::dpi(dpi.round() as u16));
            // Converted straight from the RGBA pixels, without another copy of them (R17 at the scroll cap).
            let rgb: image::RgbImage = image.convert();
            encoder.encode_image(&rgb).ok()?;
        }
    }
    Some(out)
}

/// A PNG's density in dots per inch, from its `pHYs` chunk.
fn png_dpi(bytes: &[u8]) -> Option<f64> {
    let reader = png::Decoder::new(Cursor::new(bytes)).read_info().ok()?;
    let dims = reader.info().pixel_dims?;
    (dims.unit == png::Unit::Meter && dims.xppu > 0)
        .then(|| f64::from(dims.xppu) / INCHES_PER_METRE)
}

/// A JPEG's density in dots per inch, from its JFIF header.
fn jpeg_dpi(bytes: &[u8]) -> Option<f64> {
    // FF D8, FF E0, length (2), "JFIF\0", version (2), units, X density (2), Y density (2).
    let header = bytes.get(0..18)?;
    if header.get(0..4)? != [0xFF, 0xD8, 0xFF, 0xE0] || header.get(6..11)? != b"JFIF\0" {
        return None;
    }
    let units = *header.get(13)?;
    let x = f64::from(u16::from_be_bytes([*header.get(14)?, *header.get(15)?]));
    match units {
        1 if x > 0.0 => Some(x),
        2 if x > 0.0 => Some(x * 2.54),
        _ => None,
    }
}

/// A saved shot and its scale, from its density, to the nearest quarter; none is `fallback_scale` (this PC's).
pub fn load(bytes: &[u8], fallback_scale: f64) -> Option<(RgbaImage, f64)> {
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .to_rgba8();
    let dpi = png_dpi(bytes).or_else(|| jpeg_dpi(bytes));
    let scale = dpi.map_or(fallback_scale, |dpi| ((dpi / BASE_DPI) * 4.0).round() / 4.0);
    Some((image, scale.max(1.0)))
}

/// `image` scaled by `factor` (one over the scale turns full resolution into one-to-one), never smaller than 1×1.
pub fn scaled(image: &RgbaImage, factor: f64) -> RgbaImage {
    let side = |n: u32| ((f64::from(n) * factor).round() as u32).max(1);
    imageops::resize(
        image,
        side(image.width()),
        side(image.height()),
        imageops::FilterType::Triangle,
    )
}

/// The pixel size of encoded image bytes.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba([51, 77, 179, 255]))
    }

    #[test]
    fn a_shots_density_says_its_scale() {
        let png = data(&plain(40, 20), Format::Png, 1.5).unwrap_or_default();
        assert_eq!(size(&png), Some((40, 20)));
        assert_eq!(load(&png, 1.0).map(|(_, scale)| scale), Some(1.5));
        let jpeg = data(&plain(40, 20), Format::Jpeg, 2.0).unwrap_or_default();
        assert_eq!(size(&jpeg), Some((40, 20)));
        assert_eq!(load(&jpeg, 1.0).map(|(_, scale)| scale), Some(2.0));
        // Without a density: this PC's scale.
        let mut bare = Vec::new();
        let _ = plain(4, 4).write_to(
            &mut std::io::Cursor::new(&mut bare),
            image::ImageFormat::Png,
        );
        assert_eq!(load(&bare, 1.25).map(|(_, scale)| scale), Some(1.25));
        assert_eq!(load(b"not an image", 1.0), None);
    }

    #[test]
    fn scaling_halves_the_pixels() {
        let half = scaled(&plain(200, 100), 0.5);
        assert_eq!(half.dimensions(), (100, 50));
        assert_eq!(
            scaled(&plain(3, 3), 0.1).dimensions(),
            (1, 1),
            "never empty"
        );
    }
}
