//! Samo's tokens. Samo is a Talvyn product, so it wears the family's nine colours and mixes them the way the
//! Mesh Cells style does (`company/brand/product-ui-mesh-cells.md`, `color-mix(in srgb, …)`, light/dark pairs).
//! Its own layer is in `apps/samo-gpui/DESIGN.md`: the ring, the readout (a dark instrument window with
//! phosphor figures), the probe, and a denser, business-like scale. No hex appears outside this file.

use crate::appearance::{Appearance, Mode, Style};
use gpui::{
    App, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Hsla, Rgba, WindowAppearance,
};
use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};
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
    pub shade_3: Hsla,
    pub shade_4: Hsla,
    pub backdrop: Hsla,
    /// The readout: a recessed instrument window with phosphor figures. It follows the theme: near-black glass with
    /// pale-cobalt figures in dark, a light recessed panel with Cobalt figures in light.
    pub glass: Hsla,
    pub glass_line: Hsla,
    pub glass_text: Hsla,
    pub glass_text_2: Hsla,
    pub phosphor: Hsla,
    pub phosphor_off: Hsla,
    /// A wrong figure on the readout: Signal on dark glass, a deeper orange on the light panel so it still reads.
    pub glass_signal: Hsla,
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

fn light(t: Tone) -> Palette {
    let card = WHITE;
    Palette {
        frame: h(WHITE),
        stage: h(mix(t.ink, WHITE, 0.035 * t.depth)),
        shell: h(mix(t.ink, WHITE, 0.06 * t.depth)),
        card: h(card),
        card_hover: h(mix(t.ink, card, 0.05)),
        text: h(t.ink),
        text_2: h(mix(t.ink, t.tint, 0.72)),
        hairline: h(fade(t.ink, 0.09)),
        hairline_strong: h(fade(t.ink, 0.18)),
        field_line: h(mix(t.ink, WHITE, 0.55)),
        hover: h(fade(t.ink, 0.05)),
        accent: h(BLUE),
        accent_text: h(BLUE),
        accent_wash: h(mix(BLUE, WHITE, 0.07)),
        accent_line: h(mix(BLUE, WHITE, 0.16)),
        signal: h(ORANGE),
        signal_text: h(ORANGE_INK),
        signal_wash: h(mix(ORANGE, WHITE, 0.13)),
        signal_line: h(mix(ORANGE, WHITE, 0.28)),
        primary_bg: h(t.ink),
        primary_fg: h(WHITE),
        primary_hover: h(mix(t.ink, WHITE, 0.84)),
        dot: h(fade(t.ink, 0.11)),
        shade_1: h(fade(t.ink, 0.07)),
        shade_3: h(fade(t.ink, 0.16)),
        shade_4: h(fade(t.ink, 0.34)),
        backdrop: h(fade(t.ink, 0.30)),
        // A recessed panel at the shell's tone, with Cobalt figures lit on it.
        glass: h(mix(t.ink, WHITE, 0.06 * t.depth)),
        glass_line: h(fade(t.ink, 0.14)),
        glass_text: h(t.ink),
        glass_text_2: h(mix(t.ink, t.tint, 0.72)),
        phosphor: h(BLUE),
        phosphor_off: h(fade(BLUE, 0.14)),
        glass_signal: h(ORANGE_INK),
        thumb: h(WHITE),
        thumb_face: h(t.ink),
        cells: CellColors {
            mascot: CellTint {
                o: h(BLACK),
                f: h(t.ink),
                h: h(mix(t.ink, WHITE, 0.62)),
                s: h(mix(t.ink, BLACK, 0.55)),
                v: h(t.ink),
                d: h(t.ink),
                eye: h(WHITE),
                led: h(WHITE),
                sign: h(t.ink),
            },
            agent: CellTint {
                o: h(mix(BLUE_DEEP, t.ink, 0.72)),
                f: h(BLUE),
                h: h(mix(BLUE_LIGHT, BLUE, 0.5)),
                s: h(BLUE_DEEP),
                v: h(mix(t.ink, BLUE_DEEP, 0.82)),
                d: h(mix(BLUE_LIGHT, BLUE, 0.46)),
                eye: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                led: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                sign: h(BLUE_LIGHT),
            },
            agent_asleep_eye: h(fade(BLUE_LIGHT, 0.55)),
            agent_asleep_led: h(fade(BLUE_LIGHT, 0.30)),
            device: CellTint {
                o: h(mix(t.ink, WHITE, 0.55)),
                f: h(mix(t.ink, WHITE, 0.12)),
                h: h(WHITE),
                s: h(mix(t.ink, WHITE, 0.24)),
                v: h(mix(t.ink, WHITE, 0.74)),
                d: h(mix(t.ink, WHITE, 0.74)),
                eye: h(WHITE),
                led: h(BLUE),
                sign: h(BLUE),
            },
        },
    }
}

fn dark(t: Tone) -> Palette {
    let t = Tone {
        ink: t.dark_ink,
        ..t
    };
    let card = mix(t.ink, BLACK, 0.68);
    let text = mix(WHITE, t.ink, 0.92);
    Palette {
        frame: h(mix(t.ink, BLACK, 0.26)),
        stage: h(mix(t.ink, BLACK, 0.42)),
        shell: h(mix(t.ink, BLACK, 0.54)),
        card: h(card),
        card_hover: h(mix(text, card, 0.05)),
        text: h(text),
        text_2: h(mix(WHITE, t.ink, 0.62)),
        hairline: h(fade(WHITE, 0.09)),
        hairline_strong: h(fade(WHITE, 0.17)),
        field_line: h(mix(WHITE, t.ink, 0.44)),
        hover: h(fade(WHITE, 0.06)),
        accent: h(mix(BLUE, BLUE_LIGHT, 0.38)),
        accent_text: h(BLUE_LIGHT),
        accent_wash: h(mix(BLUE, card, 0.24)),
        accent_line: h(mix(BLUE_LIGHT, card, 0.22)),
        signal: h(ORANGE),
        signal_text: h(mix(ORANGE, WHITE, t.signal)),
        signal_wash: h(mix(ORANGE, card, 0.16)),
        signal_line: h(mix(ORANGE, card, 0.34)),
        primary_bg: h(text),
        primary_fg: h(t.ink),
        primary_hover: h(mix(text, card, 0.86)),
        dot: h(fade(WHITE, 0.09)),
        shade_1: h(fade(BLACK, 0.40)),
        shade_3: h(fade(BLACK, 0.55)),
        shade_4: h(fade(BLACK, 0.70)),
        backdrop: h(fade(BLACK, 0.50)),
        glass: h(mix(t.ink, BLACK, 0.18)),
        glass_line: h(fade(WHITE, 0.12)),
        glass_text: h(WHITE),
        glass_text_2: h(mix(WHITE, mix(t.ink, BLACK, 0.18), 0.58)),
        phosphor: h(BLUE_LIGHT),
        phosphor_off: h(fade(BLUE_LIGHT, 0.10)),
        glass_signal: h(ORANGE),
        thumb: h(WHITE),
        thumb_face: h(t.ink),
        cells: CellColors {
            mascot: CellTint {
                o: h(mix(WHITE, t.ink, 0.62)),
                f: h(mix(WHITE, t.ink, 0.30)),
                h: h(mix(WHITE, t.ink, 0.54)),
                s: h(mix(WHITE, t.ink, 0.18)),
                v: h(t.ink),
                d: h(t.ink),
                eye: h(WHITE),
                led: h(WHITE),
                sign: h(WHITE),
            },
            agent: CellTint {
                o: h(mix(BLUE_DEEP, BLACK, 0.70)),
                f: h(mix(BLUE, BLUE_LIGHT, 0.62)),
                h: h(mix(BLUE_LIGHT, BLUE, 0.62)),
                s: h(mix(BLUE, BLUE_DEEP, 0.70)),
                v: h(mix(t.ink, BLACK, 0.70)),
                d: h(mix(BLUE_LIGHT, BLUE, 0.74)),
                eye: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                led: h(mix(WHITE, BLUE_LIGHT, 0.78)),
                sign: h(BLUE_LIGHT),
            },
            agent_asleep_eye: h(fade(BLUE_LIGHT, 0.55)),
            agent_asleep_led: h(fade(BLUE_LIGHT, 0.30)),
            device: CellTint {
                o: h(mix(WHITE, t.ink, 0.55)),
                f: h(mix(WHITE, t.ink, 0.26)),
                h: h(mix(WHITE, t.ink, 0.40)),
                s: h(mix(WHITE, t.ink, 0.14)),
                v: h(mix(BLACK, t.ink, 0.40)),
                d: h(mix(BLACK, t.ink, 0.40)),
                eye: h(WHITE),
                led: h(BLUE),
                sign: h(BLUE),
            },
        },
    }
}

/// What a style changes. Cobalt, Signal and the characters' own colours are the family's, the same in every style,
/// so Cobalt still means "selected" and "Samo suggests" everywhere; a style only moves the greys.
#[derive(Clone, Copy)]
struct Tone {
    /// The neutral light mixes its text and lines from.
    ink: C,
    /// The neutral dark builds its surfaces from: the lighter it is, the lighter the charcoal.
    dark_ink: C,
    /// What the secondary text leans toward in light.
    tint: C,
    /// How far the light stage and shell sit from white (1 is Cobalt's: a trace of grey).
    depth: f32,
    /// How much orange stays in Signal text in dark (the rest is white): lighter surfaces need lighter text.
    signal: f32,
}

fn tone(style: Style) -> Tone {
    match style {
        Style::Cobalt => Tone {
            ink: INK,
            dark_ink: INK,
            tint: BLUE_LIGHT,
            depth: 1.0,
            signal: 0.70,
        },
        // True greys (only black and white, mixed), and more of them: in light a grey canvas the white cards stand
        // out from, in dark a charcoal instead of near-black.
        Style::Graphite => Tone {
            ink: mix(WHITE, BLACK, 0.165),
            dark_ink: mix(WHITE, BLACK, 0.29),
            tint: mix(WHITE, BLACK, 0.8),
            depth: 2.4,
            signal: 0.62,
        },
    }
}

/// Every style in light, then in dark, in `Style::ALL`'s order (see `palette_index`).
static PALETTES: [LazyLock<Palette>; 4] = [
    LazyLock::new(|| light(tone(Style::Cobalt))),
    LazyLock::new(|| dark(tone(Style::Cobalt))),
    LazyLock::new(|| light(tone(Style::Graphite))),
    LazyLock::new(|| dark(tone(Style::Graphite))),
];

fn palette_index(style: Style, dark: bool) -> usize {
    Style::ALL.iter().position(|s| *s == style).unwrap_or(0) * 2 + usize::from(dark)
}

/// A style's palette in light or dark, whatever is current: Settings draws each style's swatch with it.
pub fn palette_for(style: Style, dark: bool) -> &'static Palette {
    &PALETTES[palette_index(style, dark)]
}

thread_local! {
    static IS_DARK: Cell<bool> = const { Cell::new(false) };
}

// What you chose in Settings → Appearance, as positions in `Mode::ALL` and `Style::ALL`. Atomics, not a
// thread-local: Settings sets them on the main thread, but every Samo window reads them as it draws.
static MODE: AtomicU8 = AtomicU8::new(0);
static STYLE: AtomicU8 = AtomicU8::new(0);

/// Takes what was chosen (at launch from `appearance.txt`, and again when you change it in Settings).
pub fn set_appearance(a: Appearance) {
    let at = |found: Option<usize>| u8::try_from(found.unwrap_or(0)).unwrap_or(0);
    MODE.store(
        at(Mode::ALL.iter().position(|m| *m == a.mode)),
        Ordering::Relaxed,
    );
    STYLE.store(
        at(Style::ALL.iter().position(|s| *s == a.style)),
        Ordering::Relaxed,
    );
}

/// What was chosen: Settings marks it. (`SAMO_THEME` and `SAMO_STYLE` still win over it on screen.)
pub fn appearance() -> Appearance {
    Appearance {
        mode: Mode::ALL
            .get(usize::from(MODE.load(Ordering::Relaxed)))
            .copied()
            .unwrap_or_default(),
        style: Style::ALL
            .get(usize::from(STYLE.load(Ordering::Relaxed)))
            .copied()
            .unwrap_or_default(),
    }
}

/// `SAMO_THEME=light|dark` pins light or dark whatever Settings says (for screenshots and tests); anything else
/// leaves it to Settings, which by default follows the system, as Mesh's does.
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

/// `SAMO_STYLE=cobalt|graphite` pins a style the same way.
fn pinned_style() -> Option<Style> {
    static PINNED: LazyLock<Option<Style>> = LazyLock::new(|| {
        Style::from_key(
            std::env::var("SAMO_STYLE")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
        )
    });
    *PINNED
}

/// Light or dark: what's pinned, else what you chose, else (if you chose "System") what the system says.
pub fn resolve_dark(pinned: Option<bool>, mode: Mode, system_dark: bool) -> bool {
    pinned.unwrap_or(match mode {
        Mode::System => system_dark,
        Mode::Light => false,
        Mode::Dark => true,
    })
}

/// The first thing every Samo window does as it draws (the window, the menu bar's panel, a pop-up): settle light
/// or dark from the window's own appearance, so a change in Settings or in System Settings shows at once.
pub fn follow(system: WindowAppearance) {
    let system_dark = matches!(
        system,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    );
    set_dark(resolve_dark(pinned_dark(), appearance().mode, system_dark));
}

/// Set once per frame by `follow`.
pub fn set_dark(dark: bool) {
    IS_DARK.with(|d| d.set(dark));
}

pub fn is_dark() -> bool {
    IS_DARK.with(Cell::get)
}

/// The style in use: pinned, else the chosen one.
pub fn style() -> Style {
    pinned_style().unwrap_or_else(|| appearance().style)
}

/// The palette for the current appearance.
pub fn p() -> &'static Palette {
    palette_for(style(), is_dark())
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
    // Menlo is the Mac's; Windows has Cascadia Mono (Windows 11) or Consolas.
    let mono_fallback = if cfg!(windows) && has("Cascadia Mono") {
        "Cascadia Mono"
    } else {
        MONO_FALLBACK
    };
    let _ = FACES.set(Faces {
        display: pick("Cabinet Grotesk", ".SystemUIFont"),
        body: pick("Switzer", ".SystemUIFont"),
        mono: pick("IBM Plex Mono", mono_fallback),
        mono_medium: if has("IBM Plex Mono Medium") {
            "IBM Plex Mono Medium"
        } else {
            pick("IBM Plex Mono", mono_fallback)
        },
    });
}

/// The OS's own monospace face, for when IBM Plex Mono isn't there.
const MONO_FALLBACK: &str = if cfg!(windows) { "Consolas" } else { "Menlo" };

fn faces() -> &'static Faces {
    FACES.get_or_init(|| Faces {
        display: ".SystemUIFont",
        body: ".SystemUIFont",
        mono: MONO_FALLBACK,
        mono_medium: MONO_FALLBACK,
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
    crate::places::home_dir()
}

/// Shortens `$HOME/...` to `~/...` for display.
pub fn display_path(path: &str) -> String {
    crate::places::display_path_from(path, &home_dir())
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

    // ------------------------------------------------------------ appearance and styles

    #[test]
    fn light_or_dark_comes_from_the_pin_then_your_choice_then_the_system() {
        use Mode::*;
        // "System" follows the system whichever way it goes.
        assert!(resolve_dark(None, System, true));
        assert!(!resolve_dark(None, System, false));
        // A fixed mode ignores the system.
        assert!(!resolve_dark(None, Light, true));
        assert!(resolve_dark(None, Dark, false));
        // SAMO_THEME beats everything.
        assert!(resolve_dark(Some(true), Light, false));
        assert!(!resolve_dark(Some(false), Dark, true));
    }

    #[test]
    fn what_you_choose_is_what_is_read_back() {
        let before = appearance();
        for mode in Mode::ALL {
            for style in Style::ALL {
                let a = Appearance { mode, style };
                set_appearance(a);
                assert_eq!(appearance(), a);
            }
        }
        set_appearance(before);
    }

    #[test]
    fn cobalt_is_still_exactly_what_samo_shipped() {
        let (light, dark) = (
            palette_for(Style::Cobalt, false),
            palette_for(Style::Cobalt, true),
        );
        // The values the palettes had before styles existed (see `mixes_match_css_color_mix`).
        assert_eq!(light.stage, h(mix(INK, WHITE, 0.035)));
        assert_eq!(light.text, h(INK));
        assert_eq!(light.text_2, h(mix(INK, BLUE_LIGHT, 0.72)));
        assert_eq!(dark.card, h(mix(INK, BLACK, 0.68)));
        assert_eq!(dark.text_2, h(mix(WHITE, INK, 0.62)));
        assert_eq!(light.accent, h(BLUE));
    }

    #[test]
    fn each_style_has_its_own_light_and_dark() {
        for dark in [false, true] {
            assert_ne!(
                palette_for(Style::Cobalt, dark).stage,
                palette_for(Style::Graphite, dark).stage,
                "the styles differ (dark = {dark})"
            );
        }
        for style in Style::ALL {
            assert_ne!(
                palette_for(style, false).card,
                palette_for(style, true).card,
                "{style:?}: light and dark differ"
            );
        }
    }

    /// Graphite is colourless where Cobalt is slate: every surface and text colour has equal red, green and blue.
    #[test]
    fn graphite_has_no_tint_in_its_neutrals() {
        for dark in [false, true] {
            let pal = palette_for(Style::Graphite, dark);
            for (name, colour) in [
                ("frame", pal.frame),
                ("stage", pal.stage),
                ("shell", pal.shell),
                ("card", pal.card),
                ("card_hover", pal.card_hover),
                ("text", pal.text),
                ("text_2", pal.text_2),
                ("field_line", pal.field_line),
                ("glass", pal.glass),
                ("primary_bg", pal.primary_bg),
            ] {
                let c = Rgba::from(colour);
                assert!(
                    (c.r - c.g).abs() < 0.004 && (c.g - c.b).abs() < 0.004,
                    "Graphite {name} (dark = {dark}) is tinted: {c:?}"
                );
            }
        }
        // And Cobalt's slate really is tinted, so the two are not the same grey.
        let c = Rgba::from(palette_for(Style::Cobalt, false).text_2);
        assert!(c.b - c.r > 0.05);
    }

    /// The readout follows the theme (a light panel in light, near-black glass in dark), is always a little darker
    /// than the stage it sits in so it reads as recessed, and a wrong figure on it is readable on either.
    #[test]
    fn the_readout_follows_the_theme_and_is_recessed() {
        let sum = |c: Hsla| {
            let c = Rgba::from(c);
            c.r + c.g + c.b
        };
        for style in Style::ALL {
            let (light, dark) = (palette_for(style, false), palette_for(style, true));
            assert!(
                sum(light.glass) > sum(dark.glass) + 1.0,
                "{style:?}: the readout is not lighter in light than in dark"
            );
            for (name, pal) in [("light", light), ("dark", dark)] {
                assert!(
                    sum(pal.glass) < sum(pal.stage),
                    "{style:?} {name}: the readout is not darker than its stage"
                );
                let ratio = contrast(pal.glass_signal, pal.glass);
                assert!(
                    ratio >= 3.0,
                    "{style:?} {name}: a wrong figure is {ratio:.2}:1 on the readout, under 3:1"
                );
            }
        }
    }

    /// WCAG relative luminance and contrast ratio of two opaque colours.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let lum = |c: Hsla| {
            let c = Rgba::from(c);
            let lin = |v: f32| {
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
        };
        let (la, lb) = (lum(a), lum(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// The ratio itself is right (so the test below can fail): black on white is 21:1, and a mid grey on white
    /// is far under 4.5:1.
    #[test]
    fn the_contrast_measure_measures() {
        assert!((contrast(h(BLACK), h(WHITE)) - 21.0).abs() < 0.01);
        assert!(contrast(h(mix(WHITE, BLACK, 0.7)), h(WHITE)) < 3.0);
        assert!((contrast(h(WHITE), h(WHITE)) - 1.0).abs() < 0.001);
    }

    /// Every pair of text and the surface it is drawn on reads at WCAG AA (4.5:1), in all four palettes.
    #[test]
    fn text_is_readable_on_what_it_sits_on_in_every_palette() {
        for style in Style::ALL {
            for dark in [false, true] {
                let pal = palette_for(style, dark);
                let pairs = [
                    ("text on card", pal.text, pal.card),
                    ("text on stage", pal.text, pal.stage),
                    ("text on frame", pal.text, pal.frame),
                    ("text on shell", pal.text, pal.shell),
                    ("text_2 on card", pal.text_2, pal.card),
                    ("text_2 on stage", pal.text_2, pal.stage),
                    ("text_2 on frame", pal.text_2, pal.frame),
                    ("text_2 on shell", pal.text_2, pal.shell),
                    ("accent_text on card", pal.accent_text, pal.card),
                    ("accent_text on stage", pal.accent_text, pal.stage),
                    ("accent_text on its wash", pal.accent_text, pal.accent_wash),
                    ("signal_text on card", pal.signal_text, pal.card),
                    ("signal_text on its wash", pal.signal_text, pal.signal_wash),
                    ("primary_fg on primary_bg", pal.primary_fg, pal.primary_bg),
                    ("glass_text on glass", pal.glass_text, pal.glass),
                    ("glass_text_2 on glass", pal.glass_text_2, pal.glass),
                    ("phosphor on glass", pal.phosphor, pal.glass),
                ];
                for (name, fg, bg) in pairs {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{style:?} {} — {name} is {ratio:.2}:1, under 4.5:1",
                        if dark { "dark" } else { "light" }
                    );
                }
            }
        }
    }

    #[test]
    fn bytes_split_into_figure_and_unit() {
        assert_eq!(bytes_parts(0), ("0".to_string(), "B"));
        assert_eq!(bytes_parts(1_572_864), ("1.5".to_string(), "MB"));
        assert_eq!(format_bytes(3 * 1_073_741_824), "3.0 GB");
    }
}
