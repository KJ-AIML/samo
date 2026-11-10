//! The icon and the description a Windows program carries in its own `.exe`, read from its resources in safe Rust
//! (ADR-006 R29: Windows' icon calls hand back GDI handles that only `unsafe` can turn into pixels).
//!
//! A program's icon is the first icon group in its resources (the one Explorer shows). Its picture closest to
//! `WANT_PX` is made into an 8-bit PNG of at most `MAX_PX`, as the Mac's icons are. Its description is the
//! `FileDescription` of its version block, the name Task Manager shows ("Antimalware Service Executable").
//!
//! Only the headers, the resource tables and the one picture are read, never the whole file: some programs are
//! hundreds of megabytes.

use std::fs::File;
use std::os::windows::fs::FileExt;
use std::path::Path;

/// Icons are drawn at up to 28 px, so 42 px at 150% scaling: the smallest picture of at least this many pixels.
const WANT_PX: u32 = 48;
/// Bigger pictures are scaled down to this, so an icon is never a 256 px texture.
const MAX_PX: u32 = 64;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const RT_VERSION: u32 = 16;
/// No picture or version block is this big; a damaged file can't make Samo read gigabytes.
const MAX_RESOURCE: usize = 4 * 1024 * 1024;
/// More entries than any resource table has.
const MAX_ENTRIES: usize = 4096;

/// How far above an .exe its package's manifest may be (`WindowsApps\<package>\app\resources\x.exe`).
const PACKAGE_DEPTH: usize = 6;
/// A manifest is a few kilobytes; nothing bigger is read.
const MAX_MANIFEST: u64 = 1024 * 1024;

/// The program's icon, as PNG bytes, or none if it has none (or isn't a program). A Store app's .exe often has no
/// icon of its own: then its package's logo is taken, as the taskbar does.
pub fn icon_png(exe: &Path) -> Option<Vec<u8>> {
    let mut rgba = resource_icon(exe).or_else(|| package_logo(exe))?;
    if rgba.width() > MAX_PX || rgba.height() > MAX_PX {
        rgba =
            image::imageops::resize(&rgba, MAX_PX, MAX_PX, image::imageops::FilterType::Lanczos3);
    }
    let mut png = Vec::new();
    rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

/// The icon in the .exe's own resources.
fn resource_icon(exe: &Path) -> Option<image::RgbaImage> {
    let file = File::open(exe).ok()?;
    let resources = Resources::open(&file)?;
    let group = resources.find(RT_GROUP_ICON, None)?;
    // GRPICONDIR: reserved, type 1 (icons), count; then 14-byte entries.
    if u16_at(&group, 2)? != 1 {
        return None;
    }
    let count = usize::from(u16_at(&group, 4)?);
    let entries: Vec<&[u8]> = (0..count)
        .map_while(|i| group.get(6 + 14 * i..6 + 14 * (i + 1)))
        .collect();
    let pictures: Vec<(u32, u16)> = entries
        .iter()
        .map(|e| {
            let width = if e[0] == 0 { 256 } else { u32::from(e[0]) };
            (width, u16::from_le_bytes([e[6], e[7]]))
        })
        .collect();
    let chosen = entries.get(pick(&pictures, WANT_PX)?)?;
    let id = u16::from_le_bytes([chosen[12], chosen[13]]);
    let picture = resources.find(RT_ICON, Some(u32::from(id)))?;

    // An .ico file of just that picture: the header, one 16-byte entry (the group's 12 bytes, then where the
    // picture starts), the picture. `image` decodes both kinds a program holds, PNG and bitmap.
    let mut ico = vec![0, 0, 1, 0, 1, 0];
    ico.extend_from_slice(chosen.get(..8)?);
    ico.extend_from_slice(&u32::try_from(picture.len()).ok()?.to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&picture);
    image::load_from_memory_with_format(&ico, image::ImageFormat::Ico)
        .ok()
        .map(|i| i.to_rgba8())
}

/// A Store app's logo: the package's `AppxManifest.xml` (in a folder above the .exe) names it for each app, and the
/// package holds it in sizes (`AppList.targetsize-48_altform-unplated.png`).
fn package_logo(exe: &Path) -> Option<image::RgbaImage> {
    let (root, manifest) = exe
        .ancestors()
        .skip(1)
        .take(PACKAGE_DEPTH)
        .find_map(|dir| {
            let path = dir.join("AppxManifest.xml");
            let meta = std::fs::metadata(&path).ok()?;
            if !meta.is_file() || meta.len() > MAX_MANIFEST {
                return None;
            }
            Some((dir.to_path_buf(), std::fs::read_to_string(&path).ok()?))
        })?;
    let relative = exe
        .strip_prefix(&root)
        .ok()?
        .to_string_lossy()
        .replace('/', "\\");
    let logo = root.join(logo_of(&manifest, &relative)?.replace('\\', "/"));
    let (folder, file) = (logo.parent()?, logo.file_name()?.to_str()?);
    let (stem, ext) = file.rsplit_once('.')?;
    let variants: Vec<(u32, bool, std::path::PathBuf)> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (size, unplated) = logo_variant(&name, stem, ext)?;
            Some((size, unplated, entry.path()))
        })
        .collect();
    let (_, _, best) = variants.iter().min_by_key(|&&(size, unplated, _)| {
        let small = size < WANT_PX;
        (small, if small { u32::MAX - size } else { size }, !unplated)
    })?;
    let bytes = std::fs::read(best).ok()?;
    image::load_from_memory(&bytes).ok().map(|i| i.to_rgba8())
}

/// The logo a manifest names for the app whose `Executable` is `exe` (relative to the package, with `\`), or for
/// its first app (a package's helpers wear its logo); else the package's own.
fn logo_of(manifest: &str, exe: &str) -> Option<String> {
    let attr = |text: &str, name: &str| -> Option<String> {
        let needle = format!(" {name}=\"");
        let from = text.find(&needle)? + needle.len();
        let rest = text.get(from..)?;
        Some(rest.get(..rest.find('"')?)?.to_string())
    };
    let apps: Vec<&str> = manifest
        .match_indices("<Application")
        .filter_map(|(at, tag)| {
            let rest = manifest.get(at + tag.len()..)?;
            // `<Applications>` is the list, not an app.
            if !rest.starts_with(char::is_whitespace) {
                return None;
            }
            Some(rest.split("</Application>").next().unwrap_or(rest))
        })
        .collect();
    let app = apps
        .iter()
        .find(|a| attr(a, "Executable").is_some_and(|e| e.eq_ignore_ascii_case(exe)))
        .or_else(|| apps.first());
    app.and_then(|a| attr(a, "Square44x44Logo")).or_else(|| {
        let from = manifest.find("<Logo>")? + "<Logo>".len();
        let rest = manifest.get(from..)?;
        Some(rest.get(..rest.find("</Logo>")?)?.trim().to_string())
    })
}

/// Whether `name` is a size of logo `stem`.`ext`, and which: its size in pixels and whether it has no plate behind
/// it. High-contrast, light-taskbar and themed variants are for other settings and aren't one.
fn logo_variant(name: &str, stem: &str, ext: &str) -> Option<(u32, bool)> {
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_prefix(&stem.to_ascii_lowercase())?;
    let rest = rest.strip_suffix(&format!(".{}", ext.to_ascii_lowercase()))?;
    if rest.is_empty() {
        return Some((44, false));
    }
    let qualifiers = rest.strip_prefix('.')?;
    let (mut size, mut unplated) = (44, false);
    for q in qualifiers.split('_') {
        if let Some(n) = q.strip_prefix("targetsize-") {
            size = n.parse().ok()?;
        } else if let Some(n) = q.strip_prefix("scale-") {
            size = 44 * n.parse::<u32>().ok()? / 100;
        } else if q == "altform-unplated" {
            unplated = true;
        } else {
            // contrast-black, altform-lightunplated, theme-dark, …
            return None;
        }
    }
    Some((size, unplated))
}

/// What the program says it is (`FileDescription`), or none.
pub fn description(exe: &Path) -> Option<String> {
    let file = File::open(exe).ok()?;
    let block = Resources::open(&file)?.find(RT_VERSION, None)?;
    version_string(&block, "FileDescription")
}

/// Of an icon group's pictures (width, bits per pixel), the one to show: the smallest at least `want` wide, the
/// most colours among equals; else the biggest there is.
fn pick(pictures: &[(u32, u16)], want: u32) -> Option<usize> {
    pictures
        .iter()
        .enumerate()
        .min_by_key(|&(_, &(width, bits))| {
            let small = width < want;
            // Big enough first, then the nearest size (the biggest, among those too small), then most colours.
            (
                small,
                if small { u32::MAX - width } else { width },
                u16::MAX - bits,
            )
        })
        .map(|(i, _)| i)
}

/// The text of `key` in a version block (`VS_VERSIONINFO`): from its first string table that has it.
fn version_string(block: &[u8], key: &str) -> Option<String> {
    let root = Node::at(block, 0)?;
    if root.key != "VS_VERSION_INFO" {
        return None;
    }
    let file_info = root.children(block).find(|n| n.key == "StringFileInfo")?;
    file_info.children(block).find_map(|table| {
        let string = table.children(block).find(|n| n.key == key)?;
        let text = utf16_until_nul(block.get(string.value_at..string.end)?);
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    })
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn align4(at: usize) -> Option<usize> {
    Some(at.checked_add(3)? & !3)
}

/// UTF-16 (little-endian) up to its first NUL.
fn utf16_until_nul(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// One node of a version block: its key, where its value starts, where its children start and where it ends.
struct Node {
    key: String,
    value_at: usize,
    children_at: usize,
    end: usize,
}

impl Node {
    fn at(block: &[u8], at: usize) -> Option<Self> {
        let len = usize::from(u16_at(block, at)?);
        let value_len = usize::from(u16_at(block, at + 2)?);
        let is_text = u16_at(block, at + 4)? == 1;
        let end = at.checked_add(len)?;
        if len < 6 || end > block.len() {
            return None;
        }
        let key_bytes = block.get(at + 6..end)?;
        let key = utf16_until_nul(key_bytes);
        let key_end = at + 6 + (key.encode_utf16().count() + 1) * 2;
        let value_at = align4(key_end)?;
        let value_bytes = if is_text { value_len * 2 } else { value_len };
        let children_at = align4(value_at.checked_add(value_bytes)?)?.min(end);
        Some(Self {
            key,
            value_at: value_at.min(end),
            children_at,
            end,
        })
    }

    fn children<'a>(&self, block: &'a [u8]) -> impl Iterator<Item = Node> + 'a {
        let end = self.end;
        let mut at = Some(self.children_at);
        std::iter::from_fn(move || {
            let here = align4(at?)?;
            if here >= end {
                return None;
            }
            let node = Node::at(block, here)?;
            // A node that doesn't move on would loop for ever.
            at = (node.end > here).then_some(node.end);
            Some(node)
        })
    }
}

/// A program's resources: where its sections are in the file, and where its resource tables start.
struct Resources<'a> {
    file: &'a File,
    /// (virtual address, size, where in the file)
    sections: Vec<(u32, u32, u32)>,
    root: u32,
}

impl<'a> Resources<'a> {
    fn open(file: &'a File) -> Option<Self> {
        let read = |at: u64, len: usize| read_at(file, at, len);
        let dos = read(0, 64)?;
        if dos.get(..2)? != b"MZ" {
            return None;
        }
        let pe = u64::from(u32_at(&dos, 0x3c)?);
        let head = read(pe, 24)?;
        if head.get(..4)? != b"PE\0\0" {
            return None;
        }
        let sections = usize::from(u16_at(&head, 6)?).min(96);
        let optional_len = usize::from(u16_at(&head, 20)?);
        let optional = read(pe + 24, optional_len)?;
        // Where the data directories start: PE32 and PE32+ lay their optional header out differently.
        let directories = match u16_at(&optional, 0)? {
            0x10b => 96,
            0x20b => 112,
            _ => return None,
        };
        if u32_at(&optional, directories - 4)? <= 2 {
            return None;
        }
        let root = u32_at(&optional, directories + 16)?;
        if root == 0 {
            return None;
        }
        let table = read(pe + 24 + optional_len as u64, sections * 40)?;
        let sections = table
            .as_chunks::<40>()
            .0
            .iter()
            .filter_map(|s| {
                let s = s.as_slice();
                let size = u32_at(s, 8)?.max(u32_at(s, 16)?);
                Some((u32_at(s, 12)?, size, u32_at(s, 20)?))
            })
            .collect();
        Some(Self {
            file,
            sections,
            root,
        })
    }

    /// `len` bytes at virtual address `rva`.
    fn read(&self, rva: u32, len: usize) -> Option<Vec<u8>> {
        if len > MAX_RESOURCE {
            return None;
        }
        let &(va, _, raw) = self
            .sections
            .iter()
            .find(|&&(va, size, _)| rva >= va && rva - va < size)?;
        read_at(self.file, u64::from(raw) + u64::from(rva - va), len)
    }

    /// The entries of the resource table at `offset` (from where the tables start): (id, or none if named;
    /// what it points at; whether that is another table).
    fn entries(&self, offset: u32) -> Option<Vec<(Option<u32>, u32, bool)>> {
        let at = self.root.checked_add(offset)?;
        let head = self.read(at, 16)?;
        let count = usize::from(u16_at(&head, 12)?) + usize::from(u16_at(&head, 14)?);
        if count > MAX_ENTRIES {
            return None;
        }
        let raw = self.read(at.checked_add(16)?, count * 8)?;
        Some(
            raw.as_chunks::<8>()
                .0
                .iter()
                .filter_map(|e| {
                    let e = e.as_slice();
                    let (name, target) = (u32_at(e, 0)?, u32_at(e, 4)?);
                    let id = (name & 0x8000_0000 == 0).then_some(name);
                    Some((id, target & 0x7fff_ffff, target & 0x8000_0000 != 0))
                })
                .collect(),
        )
    }

    /// The bytes of resource `kind` numbered `id` (or its first, as Explorer takes a program's first icon), in its
    /// first language.
    fn find(&self, kind: u32, id: Option<u32>) -> Option<Vec<u8>> {
        let types = self.entries(0)?;
        let &(_, table, is_table) = types.iter().find(|(t, _, _)| *t == Some(kind))?;
        if !is_table {
            return None;
        }
        let names = self.entries(table)?;
        let mut node = *match id {
            Some(id) => names.iter().find(|(n, _, _)| *n == Some(id))?,
            None => names.first()?,
        };
        // A name's table holds its languages, and a language points at the data.
        for _ in 0..2 {
            if !node.2 {
                break;
            }
            node = *self.entries(node.1)?.first()?;
        }
        if node.2 {
            return None;
        }
        let data = self.read(self.root.checked_add(node.1)?, 16)?;
        let size = usize::try_from(u32_at(&data, 4)?).ok()?;
        self.read(u32_at(&data, 0)?, size)
    }
}

/// Exactly `len` bytes at `at`, or none.
fn read_at(file: &File, at: u64, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0; len];
    let mut done = 0;
    while done < len {
        match file.seek_read(&mut buf[done..], at + done as u64) {
            Ok(0) | Err(_) => return None,
            Ok(n) => done += n,
        }
    }
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(png: &[u8]) -> Result<image::RgbaImage, String> {
        image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map(|i| i.to_rgba8())
            .map_err(|e| e.to_string())
    }

    fn not_blank(rgba: &image::RgbaImage) -> bool {
        rgba.pixels()
            .any(|p| p[3] > 200 && (p[0] | p[1] | p[2]) > 0)
    }

    /// Every Windows has Explorer: a 64-bit program (PE32+).
    #[test]
    fn explorers_icon_is_a_small_png_that_isnt_blank() -> Result<(), String> {
        let png = icon_png(Path::new("C:/Windows/explorer.exe")).ok_or("no icon")?;
        assert!(png.starts_with(b"\x89PNG"));
        let rgba = decode(&png)?;
        assert!(
            (WANT_PX..=MAX_PX).contains(&rgba.width()),
            "{} px",
            rgba.width()
        );
        assert!(not_blank(&rgba), "the icon is blank");
        Ok(())
    }

    /// A 32-bit program (PE32) has its headers in another layout.
    #[test]
    fn a_32_bit_programs_icon_is_read_too() -> Result<(), String> {
        let png = icon_png(Path::new("C:/Windows/SysWOW64/cmd.exe")).ok_or("no icon")?;
        assert!(not_blank(&decode(&png)?));
        Ok(())
    }

    #[test]
    fn a_program_says_what_it_is() {
        let what = description(Path::new("C:/Windows/explorer.exe")).unwrap_or_default();
        assert!(!what.trim().is_empty(), "no description");
        assert!(!what.contains('\0'));
    }

    #[test]
    fn what_is_not_a_program_has_no_icon() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut header = vec![0u8; 64];
        header[..2].copy_from_slice(b"MZ");
        header[0x3c..0x40].copy_from_slice(&5_000u32.to_le_bytes()); // points past the end
        for (name, bytes) in [
            ("empty.exe", Vec::new()),
            ("text.exe", b"not a program at all".to_vec()),
            ("cut.exe", header),
            (
                "noise.exe",
                (0..4096u32).map(|i| (i * 7919 % 251) as u8).collect(),
            ),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            assert_eq!(icon_png(&path), None, "{name}");
            assert_eq!(description(&path), None, "{name}");
        }
        assert_eq!(icon_png(&dir.path().join("missing.exe")), None);
        Ok(())
    }

    fn solid_png(path: &Path, px: u32, rgb: [u8; 3]) -> Result<(), String> {
        let img = image::RgbaImage::from_pixel(px, px, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        img.save_with_format(path, image::ImageFormat::Png)
            .map_err(|e| e.to_string())
    }

    /// A Store app (Arc, from `WindowsApps`) has no icon in its .exe: its logo is a file of its package, named in
    /// `AppxManifest.xml`, in sizes. The one just big enough without a plate behind it is taken, as the taskbar does.
    #[test]
    fn a_packaged_apps_icon_is_its_packages_logo() -> Result<(), String> {
        let package = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = package.path();
        std::fs::create_dir_all(root.join("app")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("Assets")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("app").join("Other.exe"), b"").map_err(|e| e.to_string())?;
        std::fs::write(root.join("app").join("Thing.exe"), b"").map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("AppxManifest.xml"),
            r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
  <Properties><Logo>Assets\StoreLogo.png</Logo></Properties>
  <Applications>
    <Application Id="Other" Executable="app\Other.exe">
      <uap:VisualElements DisplayName="Other" Square44x44Logo="Assets\OtherLogo.png" />
    </Application>
    <Application Id="Thing" Executable="app\Thing.exe">
      <uap:VisualElements DisplayName="Thing" Square150x150Logo="Assets\Big.png" Square44x44Logo="Assets\AppList.png" />
    </Application>
  </Applications>
</Package>"#,
        )
        .map_err(|e| e.to_string())?;
        let assets = root.join("Assets");
        solid_png(&assets.join("AppList.targetsize-16.png"), 16, [255, 0, 0])?;
        solid_png(&assets.join("AppList.targetsize-48.png"), 48, [0, 0, 255])?;
        solid_png(
            &assets.join("AppList.targetsize-48_altform-unplated.png"),
            48,
            [0, 200, 0],
        )?;
        solid_png(
            &assets.join("AppList.targetsize-48_altform-lightunplated.png"),
            48,
            [9, 9, 9],
        )?;
        solid_png(&assets.join("AppList.scale-200.png"), 88, [255, 255, 0])?;
        solid_png(&assets.join("OtherLogo.png"), 44, [255, 0, 255])?;

        let rgba = decode(&icon_png(&root.join("app").join("Thing.exe")).ok_or("no icon")?)?;
        assert_eq!(rgba.width(), 48);
        assert_eq!(
            rgba.get_pixel(10, 10).0,
            [0, 200, 0, 255],
            "48 px, unplated"
        );
        // Each app of a package has its own logo.
        let other = decode(&icon_png(&root.join("app").join("Other.exe")).ok_or("no icon")?)?;
        assert_eq!(other.get_pixel(10, 10).0, [255, 0, 255, 255]);
        // A program in no package, with no icon of its own, has none.
        let loose = tempfile::tempdir().map_err(|e| e.to_string())?;
        std::fs::write(loose.path().join("Loose.exe"), b"").map_err(|e| e.to_string())?;
        assert_eq!(icon_png(&loose.path().join("Loose.exe")), None);
        Ok(())
    }

    #[test]
    fn the_smallest_picture_big_enough_is_chosen() {
        // 16, 32, 48 and 256 px; 48 px twice, in 8 and 32 bits.
        let pictures = [(16, 32), (48, 8), (256, 32), (32, 32), (48, 32)];
        assert_eq!(pick(&pictures, 48), Some(4), "48 px, the most colours");
        assert_eq!(pick(&pictures, 64), Some(2), "nothing between: 256 px");
        // None big enough: the biggest.
        assert_eq!(pick(&[(16, 32), (32, 32)], 48), Some(1));
        assert_eq!(pick(&[], 48), None);
    }

    /// A version block as resource compilers write it: nodes of length, value length, type, a UTF-16 key, padding
    /// to four bytes, the value, padding, then children.
    fn node(key: &str, value: &[u8], text: bool, children: &[Vec<u8>]) -> Vec<u8> {
        let pad = |v: &mut Vec<u8>| {
            while !v.len().is_multiple_of(4) {
                v.push(0);
            }
        };
        let mut out = vec![0, 0, 0, 0, 0, 0];
        out[4] = u8::from(text);
        for unit in key.encode_utf16().chain([0]) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        pad(&mut out);
        out.extend_from_slice(value);
        let value_len = if text { value.len() / 2 } else { value.len() };
        out[2..4].copy_from_slice(&(value_len as u16).to_le_bytes());
        for child in children {
            pad(&mut out);
            out.extend_from_slice(child);
        }
        let len = out.len() as u16;
        out[0..2].copy_from_slice(&len.to_le_bytes());
        out
    }

    fn text(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    #[test]
    fn a_version_block_gives_its_description() {
        let strings = node(
            "040904b0",
            &[],
            true,
            &[
                node("CompanyName", &text("The Browser Company"), true, &[]),
                node("FileDescription", &text("Arc"), true, &[]),
            ],
        );
        let block = node(
            "VS_VERSION_INFO",
            &[0xbd; 52],
            false,
            &[
                node("StringFileInfo", &[], true, &[strings]),
                node("VarFileInfo", &[], true, &[]),
            ],
        );
        assert_eq!(
            version_string(&block, "FileDescription").as_deref(),
            Some("Arc")
        );
        assert_eq!(
            version_string(&block, "CompanyName").as_deref(),
            Some("The Browser Company")
        );
        assert_eq!(version_string(&block, "ProductName"), None);
        // Cut short anywhere, it is no answer, not a panic.
        for end in 0..block.len() {
            let _ = version_string(&block[..end], "FileDescription");
        }
        assert_eq!(version_string(b"garbage", "FileDescription"), None);
    }
}
