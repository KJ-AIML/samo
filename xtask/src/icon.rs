//! `cargo run -p xtask -- windows-icon`: Samo's Windows icon, `apps/samo-gpui/assets/Samo.ico` (committed), made
//! from the Mac's `Samo.icns` so both show the same picture, at the sizes Windows draws: Explorer, the taskbar, the
//! Start menu and Alt+Tab (ADR-006).

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// The sizes in `Samo.ico`, in pixels.
pub const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// Writes `Samo.ico` beside `Samo.icns`, from the `.icns`'s largest picture scaled to each of [`SIZES`].
pub fn run(root: &Path) -> Result<PathBuf, String> {
    let assets = root.join("apps").join("samo-gpui").join("assets");
    let file = File::open(assets.join("Samo.icns")).map_err(|e| format!("Samo.icns: {e}"))?;
    let family =
        icns::IconFamily::read(BufReader::new(file)).map_err(|e| format!("Samo.icns: {e}"))?;
    let largest = family
        .available_icons()
        .into_iter()
        .max_by_key(|t| t.pixel_width())
        .ok_or("Samo.icns holds no picture")?;
    let picture = family
        .get_icon_with_type(largest)
        .map_err(|e| format!("Samo.icns: {e}"))?
        .convert_to(icns::PixelFormat::RGBA);
    let source = image::RgbaImage::from_raw(
        picture.width(),
        picture.height(),
        picture.into_data().into_vec(),
    )
    .ok_or("Samo.icns' picture isn't RGBA")?;
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let small =
            image::imageops::resize(&source, size, size, image::imageops::FilterType::Lanczos3);
        let icon = ico::IconImage::from_rgba_data(size, size, small.into_raw());
        dir.add_entry(ico::IconDirEntry::encode(&icon).map_err(|e| e.to_string())?);
    }
    let out = assets.join("Samo.ico");
    let file = File::create(&out).map_err(|e| format!("Samo.ico: {e}"))?;
    dir.write(file).map_err(|e| format!("Samo.ico: {e}"))?;
    println!("Wrote {} ({} sizes)", out.display(), SIZES.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn ico_has_the_sizes_windows_uses() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let file =
            File::open(root.join("apps/samo-gpui/assets/Samo.ico")).map_err(|e| e.to_string())?;
        let dir = ico::IconDir::read(file).map_err(|e| e.to_string())?;
        let mut sizes: Vec<u32> = dir.entries().iter().map(ico::IconDirEntry::width).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, SIZES);
        assert!(dir.entries().iter().all(|e| e.width() == e.height()));
        Ok(())
    }
}
