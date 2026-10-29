//! Samo's tokens. Samo is a Talvyn product, so it wears the family's nine colours and mixes them the way the
//! Mesh Cells style does (`company/brand/product-ui-mesh-cells.md`, `color-mix(in srgb, …)`, light/dark pairs).
//! Its own layer is in `apps/samo-gpui/DESIGN.md`: the ring, the readout (a dark instrument window with
//! phosphor figures), the probe, and a denser, business-like scale. No hex appears outside this file.

use gpui::{App, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Hsla, Rgba};
use std::cell::Cell;
use std::sync::{Arc, LazyLock, OnceLock};

// ---------------------------------------------------------------- the nine brand colours

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct C {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

const fn hex(v: u32) -> C {
    C {
        r: ((v >> 16) & 0xff) as f32 / 255.0,
        g: ((v >> 8) & 0xff) as f32 / 255.0,
        b: (v & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

const BLUE: C = hex(0x243EB5);
const BLUE_DEEP: C = hex(0x182D83);
const BLUE_LIGHT: C = hex(0xABC0EF);
const INK: C = hex(0x242A33);
const WHITE: C = hex(0xFFFFFF);
const ORANGE: C = hex(0xF15B2A);
const ORANGE_INK: C = hex(0xA83B17);
const BLACK: C = hex(0x000000);

/// `color-mix(in srgb, a p, b)` for two opaque colours.
fn mix(a: C, b: C, p: f32) -> C {
    C {
        r: a.r * p + b.r * (1.0 - p),
        g: a.g * p + b.g * (1.0 - p),
        b: a.b * p + b.b * (1.0 - p),
        a: 1.0,
    }
}

/// `color-mix(in srgb, a p, transparent)`.
fn fade(a: C, p: f32) -> C {
    C { a: p, ..a }
}

impl From<C> for Hsla {
    fn from(c: C) -> Self {
        Rgba {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
        .into()
    }
}

/// `oklch(l c h)` → sRGB, clamped into gamut. Used only for the person cells' six hues.
fn oklch(l: f32, c: f32, h_deg: f32) -> C {
    let h = h_deg.to_radians();
    let (a, b) = (c * h.cos(), c * h.sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let lin = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    let enc = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    C {
        r: enc(lin[0]),
        g: enc(lin[1]),
        b: enc(lin[2]),
        a: 1.0,
    }
}

// ---------------------------------------------------------------- surfaces, text, lines, meaning

pub struct Palette {
    pub frame: Hsla,
    pub stage: Hsla,
    pub shell: Hsla,
    pub card: Hsla,
    /// A default button under the pointer: `color-mix(text 5%, card)`.
    pub card_hover: Hsla,
    pub text: Hsla,
    pub text_2: Hsla,
    pub hairline: Hsla,
    pub hairline_strong: Hsla,
    pub field_line: Hsla,
    pub hover: Hsla,
    pub accent: Hsla,
    pub accent_text: Hsla,
    pub accent_wash: Hsla,
    pub accent_line: Hsla,
    pub signal: Hsla,
    pub signal_text: Hsla,
    pub signal_wash: Hsla,
    pub signal_line: Hsla,
    pub primary_bg: Hsla,
    pub primary_fg: Hsla,
    pub primary_hover: Hsla,
    pub dot: Hsla,
    pub shade_1: Hsla,
    pub shade_2: Hsla,
    pub shade_3: Hsla,
    pub shade_4: Hsla,
    pub backdrop: Hsla,
    /// The readout: a recessed instrument window with phosphor figures, dark in both themes.
    pub glass: Hsla,
    pub glass_line: Hsla,
    pub glass_text: Hsla,
    pub glass_text_2: Hsla,
    pub phosphor: Hsla,
    pub phosphor_off: Hsla,
    /// The switch thumb and its face, the same in both themes.
    pub thumb: Hsla,
    pub thumb_face: Hsla,
    pub cells: CellColors,
}

/// Fill per layer: outline, body, light, shade, visor/die, dither, eyes, lamp, sign.
#[derive(Clone, Copy)]
pub struct CellTint {
    pub o: Hsla,
    pub f: Hsla,
    pub h: Hsla,
    pub s: Hsla,
    pub v: Hsla,
    pub d: Hsla,
    pub eye: Hsla,
    pub led: Hsla,
    pub sign: Hsla,
}

pub struct CellColors {
    pub mascot: CellTint,
    pub agent: CellTint,
    pub agent_asleep_eye: Hsla,
    pub agent_asleep_led: Hsla,
    pub device: CellTint,
}

fn h(c: C) -> Hsla {
    c.into()
}

fn light() -> Palette {
    let card = WHITE;
    Palette {
        frame: h(WHITE),
        stage: h(mix(INK, WHITE, 0.035)),
        shell: h(mix(INK, WHITE, 0.06)),
        card: h(card),
        card_hover: h(mix(INK, card, 0.05)),
        text: h(INK),
        text_2: h(mix(INK, BLUE_LIGHT, 0.72)),
        hairline: h(fade(INK, 0.09)),
        hairline_strong: h(fade(INK, 0.18)),
        field_line: h(mix(INK, WHITE, 0.55)),
        hover: h(fade(INK, 0.05)),
        accent: h(BLUE),
        accent_text: h(BLUE),
        accent_wash: h(mix(BLUE, WHITE, 0.07)),
        accent_line: h(mix(BLUE, WHITE, 0.16)),
        signal: h(ORANGE),
        signal_text: h(ORANGE_INK),
        signal_wash: h(mix(ORANGE, WHITE, 0.13)),
        signal_line: h(mix(ORANGE, WHITE, 0.28)),
        primary_bg: h(INK),
        primary_fg: h(WHITE),
        primary_hover: h(mix(INK, WHITE, 0.84)),
        dot: h(fade(INK, 0.11)),
        shade_1: h(fade(INK, 0.07)),
        shade_2: h(fade(INK, 0.05)),
        shade_3: h(fade(INK, 0.16)),
        shade_4: h(fade(INK, 0.34)),
        backdrop: h(fade(INK, 0.30)),
        glass: h(mix(INK, BLACK, 0.72)),
        glass_line: h(fade(WHITE, 0.14)),
        glass_text: h(WHITE),
        glass_text_2: h(mix(WHITE, mix(INK, BLACK, 0.72), 0.62)),
        phosphor: h(BLUE_LIGHT),
        phosphor_off: h(fade(BLUE_LIGHT, 0.12)),
        thumb: h(WHITE),
        thumb_face: h(INK),
        cells: CellColors {
            mascot: CellTint {
                o: h(BLACK),
                f: h(INK),
                h: h(mix(INK, WHITE, 0.62)),
                s: h(mix(INK, BLACK, 0.55)),
                v: h(INK),
                d: h(INK),
                eye: h(WHITE),
                led: h(WHITE),
                sign: h(INK),
            },
            agent: CellTint {
                o: h(mix(BLUE_DEEP, INK, 0.72)),
                f: h(BLUE),
                h: h(mix(BLUE_LIGHT, BLUE, 0.5)),
                s: h(BLUE_DEEP),
                v: h(mix(INK, BLUE_DEEP, 0.82)),
                d: h(mix(BLUE_LIGHT, BLUE, 0.46)),
                eye: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                led: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                sign: h(BLUE_LIGHT),
            },
            agent_asleep_eye: h(fade(BLUE_LIGHT, 0.55)),
            agent_asleep_led: h(fade(BLUE_LIGHT, 0.30)),
            device: CellTint {
                o: h(mix(INK, WHITE, 0.55)),
                f: h(mix(INK, WHITE, 0.12)),
                h: h(WHITE),
                s: h(mix(INK, WHITE, 0.24)),
                v: h(mix(INK, WHITE, 0.74)),
                d: h(mix(INK, WHITE, 0.74)),
                eye: h(WHITE),
                led: h(BLUE),
                sign: h(BLUE),
            },
        },
    }
}

fn dark() -> Palette {
    let card = mix(INK, BLACK, 0.68);
    let text = mix(WHITE, INK, 0.92);
    Palette {
        frame: h(mix(INK, BLACK, 0.26)),
        stage: h(mix(INK, BLACK, 0.42)),
        shell: h(mix(INK, BLACK, 0.54)),
        card: h(card),
        card_hover: h(mix(text, card, 0.05)),
        text: h(text),
        text_2: h(mix(WHITE, INK, 0.62)),
        hairline: h(fade(WHITE, 0.09)),
        hairline_strong: h(fade(WHITE, 0.17)),
        field_line: h(mix(WHITE, INK, 0.44)),
        hover: h(fade(WHITE, 0.06)),
        accent: h(mix(BLUE, BLUE_LIGHT, 0.38)),
        accent_text: h(BLUE_LIGHT),
        accent_wash: h(mix(BLUE, card, 0.24)),
        accent_line: h(mix(BLUE_LIGHT, card, 0.22)),
        signal: h(ORANGE),
        signal_text: h(mix(ORANGE, WHITE, 0.70)),
        signal_wash: h(mix(ORANGE, card, 0.16)),
        signal_line: h(mix(ORANGE, card, 0.34)),
        primary_bg: h(text),
        primary_fg: h(INK),
        primary_hover: h(mix(text, card, 0.86)),
        dot: h(fade(WHITE, 0.09)),
        shade_1: h(fade(BLACK, 0.40)),
        shade_2: h(fade(BLACK, 0.30)),
        shade_3: h(fade(BLACK, 0.55)),
        shade_4: h(fade(BLACK, 0.70)),
        backdrop: h(fade(BLACK, 0.50)),
        glass: h(mix(INK, BLACK, 0.18)),
        glass_line: h(fade(WHITE, 0.12)),
        glass_text: h(WHITE),
        glass_text_2: h(mix(WHITE, mix(INK, BLACK, 0.18), 0.58)),
        phosphor: h(BLUE_LIGHT),
        phosphor_off: h(fade(BLUE_LIGHT, 0.10)),
        thumb: h(WHITE),
        thumb_face: h(INK),
        cells: CellColors {
            mascot: CellTint {
                o: h(mix(WHITE, INK, 0.62)),
                f: h(mix(WHITE, INK, 0.30)),
                h: h(mix(WHITE, INK, 0.54)),
                s: h(mix(WHITE, INK, 0.18)),
                v: h(INK),
                d: h(INK),
                eye: h(WHITE),
                led: h(WHITE),
                sign: h(WHITE),
            },
            agent: CellTint {
                o: h(mix(BLUE_DEEP, BLACK, 0.70)),
                f: h(mix(BLUE, BLUE_LIGHT, 0.62)),
                h: h(mix(BLUE_LIGHT, BLUE, 0.62)),
                s: h(mix(BLUE, BLUE_DEEP, 0.70)),
                v: h(mix(INK, BLACK, 0.70)),
                d: h(mix(BLUE_LIGHT, BLUE, 0.74)),
                eye: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                led: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                sign: h(BLUE_LIGHT),
            },
            agent_asleep_eye: h(fade(BLUE_LIGHT, 0.55)),
            agent_asleep_led: h(fade(BLUE_LIGHT, 0.30)),
            device: CellTint {
                o: h(mix(WHITE, INK, 0.55)),
                f: h(mix(WHITE, INK, 0.26)),
                h: h(mix(WHITE, INK, 0.40)),
                s: h(mix(WHITE, INK, 0.14)),
                v: h(mix(BLACK, INK, 0.40)),
                d: h(mix(BLACK, INK, 0.40)),
                eye: h(WHITE),
                led: h(BLUE),
                sign: h(BLUE),
            },
        },
    }
}

static LIGHT: LazyLock<Palette> = LazyLock::new(light);
static DARK: LazyLock<Palette> = LazyLock::new(dark);

thread_local! {
    static IS_DARK: Cell<bool> = const { Cell::new(false) };
}

/// `SAMO_THEME=light|dark` pins a theme; anything else follows the system, as Mesh's does.
pub fn pinned_dark() -> Option<bool> {
    static PINNED: LazyLock<Option<bool>> = LazyLock::new(|| {
        match std::env::var("SAMO_THEME")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "light" => Some(false),
            "dark" => Some(true),
            _ => None,
        }
    });
    *PINNED
}

/// Set once per frame from the window's appearance (the system's light or dark).
pub fn set_dark(dark: bool) {
    IS_DARK.with(|d| d.set(dark));
}

pub fn is_dark() -> bool {
    IS_DARK.with(Cell::get)
}

/// The palette for the current appearance.
pub fn p() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

/// File categories, in their fixed order. Colour follows the category, never its rank; the order keeps
/// neighbours apart for colour-blind readers (validated in both themes). Orange and red are left out: in
/// Samo they mean "wrong".
pub const CATEGORY_ORDER: [samo_primitives::FileCategory; 7] = {
    use samo_primitives::FileCategory as F;
    [
        F::Code,
        F::Documents,
        F::Archives,
        F::Media,
        F::System,
        F::Applications,
        F::Other,
    ]
};

/// A category's colour: `swatch` for marks and dots, `fill` for a bubble's body, `fill_strong` under focus.
#[derive(Clone, Copy)]
pub struct CategoryColor {
    pub swatch: Hsla,
    pub fill: Hsla,
    pub fill_strong: Hsla,
}

pub fn category_color(cat: samo_primitives::FileCategory) -> CategoryColor {
    use samo_primitives::FileCategory as F;
    let pal = p();
    let (light, dark) = match cat {
        F::Code => (0x2a78d6, 0x3987e5),
        F::Documents => (0x1baf7a, 0x199e70),
        F::Archives => (0xeda100, 0xc98500),
        F::Media => (0xe87ba4, 0xd55181),
        F::System => (0x008300, 0x2a9a2a),
        F::Applications => (0x6250d6, 0x9085e9),
        F::Other => {
            return CategoryColor {
                swatch: pal.text_2,
                fill: pal.hover,
                fill_strong: pal.hairline_strong,
            };
        }
    };
    let swatch: Hsla = hex(if is_dark() { dark } else { light }).into();
    let tint = |a: f32| Hsla { a, ..swatch };
    CategoryColor {
        swatch,
        fill: tint(if is_dark() { 0.26 } else { 0.16 }),
        fill_strong: tint(if is_dark() { 0.42 } else { 0.30 }),
    }
}

/// The two lines a live chart draws, in this fixed order: CPU then memory, in then out, read then write.
/// A Cobalt light enough to sit with the aqua at equal weight; validated as a pair in both themes (lightness
/// band, colour-blind separation, 3:1 against the card).
pub fn series_pair() -> (Hsla, Hsla) {
    let (first, second) = if is_dark() {
        (0x788fd9, 0x199e70)
    } else {
        (0x3a55c8, 0x199e70)
    };
    (hex(first).into(), hex(second).into())
}

/// A person's cell: one of six hues of equal lightness and chroma, picked by a stable hash.
pub fn person_tint(seed: &str) -> CellTint {
    const HUES: [f32; 6] = [255.0, 205.0, 155.0, 95.0, 355.0, 305.0];
    let hue = HUES[(mesh_hash(seed) % 6) as usize];
    let o = h(oklch(0.5, 0.13, hue));
    CellTint {
        o,
        f: h(oklch(0.82, 0.13, hue)),
        h: h(oklch(0.93, 0.07, hue)),
        s: h(oklch(0.7, 0.14, hue)),
        v: o,
        d: o,
        eye: h(oklch(0.27, 0.07, hue)),
        led: o,
        sign: o,
    }
}

/// Mesh's `hash`: FNV-1a then an avalanche, so a name always gets the same hue.
pub fn mesh_hash(seed: &str) -> u32 {
    let mut x: u32 = 2_166_136_261;
    for unit in seed.encode_utf16() {
        x ^= u32::from(unit);
        x = x.wrapping_mul(16_777_619);
    }
    x ^= x >> 16;
    x = x.wrapping_mul(0x045d_9f3b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x045d_9f3b);
    x ^= x >> 16;
    x
}

// ---------------------------------------------------------------- space, shape, type

pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S5: f32 = 24.0;
pub const S6: f32 = 32.0;

// An instrument is precise, not soft: Samo runs a step denser and crisper than Mesh.
pub const CONTROL_H: f32 = 36.0;
pub const CONTROL_H_COMPACT: f32 = 30.0;
pub const R_CHIP: f32 = 6.0;
pub const R_CONTROL: f32 = 8.0;
pub const R_CARD: f32 = 12.0;
pub const R_SHELL: f32 = 14.0;
pub const R_STAGE: f32 = 18.0;
pub const R_DIALOG: f32 = 20.0;
pub const SIDEBAR_W: f32 = 240.0;
pub const RAIL_W: f32 = 72.0;

pub const T_LABEL: f32 = 11.0;
pub const T_META: f32 = 12.5;
pub const T_BODY: f32 = 14.0;
pub const T_LEAD: f32 = 16.0;
pub const T_H2: f32 = 17.0;
pub const T_H1: f32 = 28.0;

/// Which of the brand faces are installed; anything missing falls back to the system's own.
struct Faces {
    display: &'static str,
    body: &'static str,
    mono: &'static str,
    mono_medium: &'static str,
}

static FACES: OnceLock<Faces> = OnceLock::new();

/// Registers the bundled IBM Plex Mono (SIL OFL 1.1) and picks the faces to use. Cabinet Grotesk and Switzer
/// (ITF Free Font License) are not redistributable, so they are used when installed and otherwise fall back.
pub fn init_fonts(cx: &mut App) {
    let _ = cx.text_system().add_fonts(vec![
        std::borrow::Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-mono-400.woff2").as_slice(),
        ),
        std::borrow::Cow::Borrowed(
            include_bytes!("../assets/fonts/ibm-plex-mono-500.woff2").as_slice(),
        ),
    ]);
    let names = cx.text_system().all_font_names();
    let has = |n: &str| names.iter().any(|x| x == n);
    let pick = |want: &'static str, fallback: &'static str| if has(want) { want } else { fallback };
    let _ = FACES.set(Faces {
        display: pick("Cabinet Grotesk", ".SystemUIFont"),
        body: pick("Switzer", ".SystemUIFont"),
        mono: pick("IBM Plex Mono", "Menlo"),
        mono_medium: if has("IBM Plex Mono Medium") {
            "IBM Plex Mono Medium"
        } else {
            pick("IBM Plex Mono", "Menlo")
        },
    });
}

fn faces() -> &'static Faces {
    FACES.get_or_init(|| Faces {
        display: ".SystemUIFont",
        body: ".SystemUIFont",
        mono: "Menlo",
        mono_medium: "Menlo",
    })
}

fn font(family: &'static str, weight: FontWeight, tnum: bool) -> Font {
    Font {
        family: family.into(),
        features: if tnum {
            FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
        } else {
            FontFeatures::default()
        },
        fallbacks: Some(FontFallbacks::from_fonts(vec![".SystemUIFont".into()])),
        weight,
        style: FontStyle::Normal,
    }
}

/// Cabinet Grotesk 700: h1, h2, the product's name.
pub fn display() -> Font {
    font(faces().display, FontWeight::BOLD, false)
}

/// Switzer: body text and controls.
pub fn body(weight: FontWeight) -> Font {
    font(faces().body, weight, true)
}

/// IBM Plex Mono: labels, tags, counts and every figure in a column.
pub fn mono(weight: FontWeight) -> Font {
    if weight >= FontWeight::MEDIUM {
        font(faces().mono_medium, FontWeight::NORMAL, false)
    } else {
        font(faces().mono, FontWeight::NORMAL, false)
    }
}

// ---------------------------------------------------------------- formatting

pub fn format_bytes(bytes: u64) -> String {
    let (value, unit) = bytes_parts(bytes);
    format!("{value} {unit}")
}

/// `(number, unit)`, so a figure can be drawn in dots beside its unit in words.
pub fn bytes_parts(bytes: u64) -> (String, &'static str) {
    const TB: f64 = 1_099_511_627_776.0;
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;
    let b = bytes as f64;
    if b >= TB {
        (format!("{:.2}", b / TB), "TB")
    } else if b >= GB {
        (format!("{:.1}", b / GB), "GB")
    } else if b >= MB {
        (format!("{:.1}", b / MB), "MB")
    } else if b >= KB {
        (format!("{:.0}", b / KB), "KB")
    } else {
        (bytes.to_string(), "B")
    }
}

pub fn format_rate(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

/// How long ago a sample was, short enough for a chart's tooltip: `now`, `34 s ago`, `12 min ago`.
pub fn format_since(secs: i64) -> String {
    match secs.max(0) {
        0..=1 => "now".to_string(),
        s @ 2..=119 => format!("{s} s ago"),
        s => format!("{} min ago", (s + 30) / 60),
    }
}

/// `120334` → `120,334`.
pub fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins = (secs % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

pub fn format_ago(then_ms: i64) -> String {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let secs = (now_ms - then_ms).max(0) / 1000;
    if secs < 45 {
        "just now".to_string()
    } else if secs < 3_600 {
        format!("{} min ago", (secs + 30) / 60)
    } else if secs < 86_400 {
        format!("{} h ago", secs / 3_600)
    } else {
        format!("{} d ago", secs / 86_400)
    }
}

pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// Shortens `$HOME/...` to `~/...` for display.
pub fn display_path(path: &str) -> String {
    let home = home_dir();
    if !home.is_empty() {
        if path == home {
            return "~".to_string();
        }
        if let Some(rest) = path.strip_prefix(&format!("{home}/")) {
            return format!("~/{rest}");
        }
    }
    path.to_string()
}

pub fn ratio(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64).clamp(0.0, 1.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_get_thousands_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_000), "1,000");
        assert_eq!(format_count(120_334), "120,334");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }

    #[test]
    fn uptime_picks_two_largest_units() {
        assert_eq!(format_uptime(59), "0m");
        assert_eq!(format_uptime(3_660), "1h 1m");
        assert_eq!(format_uptime(90_000), "1d 1h");
    }

    #[test]
    fn ratio_is_clamped_and_zero_safe() {
        assert_eq!(ratio(5, 0), 0.0);
        assert_eq!(ratio(20, 10), 1.0);
        assert!((ratio(1, 4) - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn mixes_match_css_color_mix() {
        // brand.css: --stage = color-mix(in srgb, ink 3.5%, white)
        let stage = mix(INK, WHITE, 0.035);
        assert!((stage.r * 255.0 - 247.3).abs() < 0.5);
        // a hairline keeps Ink and takes the percentage as alpha
        assert_eq!(fade(INK, 0.09).a, 0.09);
    }

    #[test]
    fn oklch_white_and_black_round_trip() {
        let w = oklch(1.0, 0.0, 0.0);
        assert!(w.r > 0.99 && w.g > 0.99 && w.b > 0.99);
        let k = oklch(0.0, 0.0, 0.0);
        assert!(k.r < 0.01 && k.g < 0.01 && k.b < 0.01);
    }

    #[test]
    fn hash_matches_mesh_cell_tsx() {
        // Reference values from Mesh's `hash()` in components/Cell.tsx, run in Node.
        assert_eq!(mesh_hash("kj"), 4_253_240_210);
        assert_eq!(mesh_hash("you@example.com"), 2_405_931_571);
        assert_eq!(mesh_hash(""), 431_477_908);
    }

    #[test]
    fn bytes_split_into_figure_and_unit() {
        assert_eq!(bytes_parts(0), ("0".to_string(), "B"));
        assert_eq!(bytes_parts(1_572_864), ("1.5".to_string(), "MB"));
        assert_eq!(format_bytes(3 * 1_073_741_824), "3.0 GB");
    }
}
