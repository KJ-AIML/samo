//! Samo's art, drawn in code (no image files): the cells on a 16 x 18 pixel grid, dot-matrix figures, LED
//! meters, the ring gauge, the dial, the measuring grid, the dither (what Samo suggests), and single-line text
//! that truncates at paint time (gpui 0.2.2's `truncate()` can paint a stale measurement inside flex rows).
//!
//! Samo's own silhouettes, not Mesh's: Samo is a round **probe** (a readout face, a probe antenna, knobs and
//! feet); the disk is a **platter**; you are a rounded square. Everything is whole pixels, and motion is
//! stepped from one clock that only ticks while Samo is scanning, so an idle Samo draws nothing new.

use crate::theme::*;
use gpui::{
    Bounds, Canvas, Div, Font, Hsla, ParentElement, PathBuilder, Pixels, Point, SharedString,
    Styled, TextRun, Window, canvas, div, fill, point, px, quad, size,
};
use std::f32::consts::{PI, TAU};
use std::sync::LazyLock;
use std::time::Instant;

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Milliseconds since the app started: the one clock every stepped animation reads.
pub fn clock_ms() -> u128 {
    EPOCH.elapsed().as_millis()
}

/// Where the probe's scan bar is: six steps across its face and back, one step per 150 ms.
pub fn scan_step() -> i32 {
    let sweep = ((clock_ms() / 150) % 12) as i32;
    if sweep <= 6 { sweep } else { 12 - sweep }
}

// ---------------------------------------------------------------- the grid

const GW: i32 = 16;
const GH: i32 = 18;
type Grid = [[bool; 16]; 18];
/// Horizontal runs `(x, y, w)`.
type Runs = Vec<(i32, i32, i32)>;

fn blank() -> Grid {
    [[false; 16]; 18]
}

fn set(g: &mut Grid, x: i32, y: i32) {
    if (0..GW).contains(&x) && (0..GH).contains(&y) {
        g[y as usize][x as usize] = true;
    }
}

fn unset(g: &mut Grid, x: i32, y: i32) {
    if (0..GW).contains(&x) && (0..GH).contains(&y) {
        g[y as usize][x as usize] = false;
    }
}

fn at(g: &Grid, x: i32, y: i32) -> bool {
    (0..GW).contains(&x) && (0..GH).contains(&y) && g[y as usize][x as usize]
}

/// A body made of centred rows: `widths[i]` pixels wide, starting at row `top`.
fn body(top: i32, widths: &[i32]) -> Grid {
    let mut g = blank();
    for (i, w) in widths.iter().enumerate() {
        let x0 = (GW - w) / 2;
        for x in x0..x0 + w {
            set(&mut g, x, top + i as i32);
        }
    }
    g
}

fn rect(g: &mut Grid, x: i32, y: i32, w: i32, h: i32) {
    for j in 0..h {
        for i in 0..w {
            set(g, x + i, y + j);
        }
    }
}

fn runs(g: &Grid) -> Runs {
    let mut out = Vec::new();
    for y in 0..GH {
        let mut x = 0;
        while x < GW {
            if !at(g, x, y) {
                x += 1;
                continue;
            }
            let mut run = 1;
            while at(g, x + run, y) {
                run += 1;
            }
            out.push((x, y, run));
            x += run;
        }
    }
    out
}

struct Layers {
    o: Runs,
    f: Runs,
    h: Runs,
    s: Runs,
    /// face window / hub / track
    v: Runs,
    /// dither: a machine's surface
    d: Runs,
    /// antenna, knobs, feet (in the outline's colour)
    a: Runs,
    led: Runs,
}

/// Outline = pixels with an empty neighbour; light = rim just inside on the upper left; shade = lower right.
fn layers(g: &Grid, v: Grid, d: Grid, a: Grid, led: Grid) -> Layers {
    let (mut o, mut f, mut hl, mut s) = (blank(), blank(), blank(), blank());
    let (mut n, mut cx, mut cy) = (0.0_f32, 0.0_f32, 0.0_f32);
    for y in 0..GH {
        for x in 0..GW {
            if at(g, x, y) {
                n += 1.0;
                cx += x as f32;
                cy += y as f32;
            }
        }
    }
    cx /= n.max(1.0);
    cy /= n.max(1.0);
    for y in 0..GH {
        for x in 0..GW {
            if at(g, x, y)
                && (!at(g, x - 1, y) || !at(g, x + 1, y) || !at(g, x, y - 1) || !at(g, x, y + 1))
            {
                set(&mut o, x, y);
            }
        }
    }
    for y in 0..GH {
        for x in 0..GW {
            if !at(g, x, y) || at(&o, x, y) {
                continue;
            }
            let upper_left = (x as f32 - cx) + (y as f32 - cy) < 0.0;
            if upper_left && (at(&o, x - 1, y) || at(&o, x, y - 1)) {
                set(&mut hl, x, y);
            } else if !upper_left && (at(&o, x + 1, y) || at(&o, x, y + 1)) {
                set(&mut s, x, y);
            } else {
                set(&mut f, x, y);
            }
        }
    }
    Layers {
        o: runs(&o),
        f: runs(&f),
        h: runs(&hl),
        s: runs(&s),
        v: runs(&v),
        d: runs(&d),
        a: runs(&a),
        led: runs(&led),
    }
}

const ROUND_13: [i32; 13] = [6, 10, 12, 14, 14, 14, 14, 14, 14, 14, 12, 10, 6];

/// Samo: a round probe. Dithered crown (a machine's surface), a readout face, a probe antenna with a lamp,
/// knobs on its sides and two feet: an instrument standing on your desk.
static PROBE: LazyLock<Layers> = LazyLock::new(|| {
    let g = body(4, &ROUND_13);
    let mut v = blank();
    rect(&mut v, 3, 8, 10, 5);
    for (x, y) in [(3, 8), (12, 8), (3, 12), (12, 12)] {
        unset(&mut v, x, y);
    }
    let mut d = blank();
    for y in 5..8 {
        for x in 1..15 {
            let dense = y < 7 && (x + y) % 2 == 0;
            let sparse = y == 7 && x % 2 == 0;
            if (dense || sparse) && at(&g, x, y) && at(&g, x - 1, y) && at(&g, x + 1, y) {
                set(&mut d, x, y);
            }
        }
    }
    let mut a = blank();
    rect(&mut a, 7, 1, 2, 3);
    rect(&mut a, 0, 9, 1, 3);
    rect(&mut a, 15, 9, 1, 3);
    rect(&mut a, 4, 17, 2, 1);
    rect(&mut a, 10, 17, 2, 1);
    let mut led = blank();
    rect(&mut led, 7, 0, 2, 1);
    layers(&g, v, d, a, led)
});

/// The disk: a platter with a track ring, a hub and a lamp.
static PLATTER: LazyLock<Layers> = LazyLock::new(|| {
    let g = body(3, &[6, 10, 12, 14, 14, 14, 14, 14, 14, 14, 14, 12, 10, 6]);
    let mut v = blank();
    let (cx, cy) = (7.5_f32, 9.5_f32);
    for y in 0..GH {
        for x in 0..GW {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            if (d - 4.0).abs() < 0.55 && at(&g, x, y) {
                set(&mut v, x, y);
            }
        }
    }
    rect(&mut v, 7, 9, 2, 2);
    let mut led = blank();
    set(&mut led, 12, 14);
    layers(&g, v, blank(), blank(), led)
});

/// You: a rounded square, solid all through (what a person decides is solid).
static PERSON: LazyLock<Layers> = LazyLock::new(|| {
    let g = body(3, &[8, 12, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 12, 8]);
    layers(&g, blank(), blank(), blank(), blank())
});

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Probe,
    Platter,
    Person,
}

/// Only ever a real state: scanning (working), done (idle), failed (blocked), stopped or unknown (asleep).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Idle,
    Working,
    Blocked,
    Asleep,
}

type Px = &'static [(i32, i32, i32, i32)];

fn eyes(shape: Shape, mood: Mood) -> (Px, Px) {
    match (shape, mood) {
        (Shape::Probe, Mood::Idle) => (&[(5, 9, 2, 2)], &[(9, 9, 2, 2)]),
        // a scan bar that sweeps the readout face
        (Shape::Probe, Mood::Working) => (&[(4, 9, 2, 3)], &[]),
        (Shape::Probe, Mood::Blocked) => (
            &[
                (4, 9, 1, 1),
                (6, 9, 1, 1),
                (5, 10, 1, 1),
                (4, 11, 1, 1),
                (6, 11, 1, 1),
            ],
            &[
                (9, 9, 1, 1),
                (11, 9, 1, 1),
                (10, 10, 1, 1),
                (9, 11, 1, 1),
                (11, 11, 1, 1),
            ],
        ),
        (Shape::Probe, Mood::Asleep) => (&[(4, 11, 8, 1)], &[]),
        (Shape::Person, Mood::Asleep) => (&[(4, 9, 3, 1)], &[(9, 9, 3, 1)]),
        (Shape::Person, _) => (&[(5, 7, 1, 3)], &[(10, 7, 1, 3)]),
        (Shape::Platter, _) => (&[], &[]),
    }
}

fn sign(shape: Shape, mood: Mood) -> Px {
    match (shape, mood) {
        (Shape::Probe, Mood::Asleep) => &[(11, 0, 3, 1), (12, 1, 1, 1), (11, 2, 3, 1)],
        (Shape::Probe, Mood::Blocked) => &[(13, 0, 1, 2), (13, 3, 1, 1)],
        _ => &[],
    }
}

/// Whose cell this is.
#[derive(Clone, Copy)]
pub enum Who {
    /// Samo's mascot: the probe in Ink, nobody's colour (empty places, errors).
    Mascot,
    /// Samo itself, at work: the probe in Cobalt.
    Samo,
    /// The Mac's disk.
    Disk,
    /// You, in your own hue.
    Person(CellTint),
}

/// One cell standing on its own. `px_size` is one grid pixel (1, 1.5, 2, 3 …).
pub fn cell(who: Who, mood: Mood, px_size: f32) -> Canvas<()> {
    let shape = match who {
        Who::Samo | Who::Mascot => Shape::Probe,
        Who::Disk => Shape::Platter,
        Who::Person(_) => Shape::Person,
    };
    // At work the probe's scan bar sweeps its face in six steps, there and back; its lamp blinks.
    let scan = scan_step();
    let lamp_dim = mood == Mood::Working && (clock_ms() / 450) % 2 == 1;

    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let pal = p();
            let mut tint = match who {
                Who::Mascot => pal.cells.mascot,
                Who::Samo => pal.cells.agent,
                Who::Disk => pal.cells.device,
                Who::Person(t) => t,
            };
            match mood {
                Mood::Blocked => {
                    tint.eye = pal.signal;
                    tint.sign = pal.signal;
                    tint.led = pal.signal;
                }
                Mood::Asleep => match shape {
                    Shape::Probe if matches!(who, Who::Samo) => {
                        tint.eye = pal.cells.agent_asleep_eye;
                        tint.led = pal.cells.agent_asleep_led;
                    }
                    Shape::Platter => tint.led = Hsla::transparent_black(),
                    _ => {}
                },
                _ => {}
            }
            let body_alpha = if mood == Mood::Asleep { 0.7 } else { 1.0 };
            let dim = |mut c: Hsla, a: f32| {
                c.a *= a;
                c
            };

            let ox = (f32::from(bounds.origin.x) * 2.0).round() / 2.0;
            let oy = (f32::from(bounds.origin.y) * 2.0).round() / 2.0;
            let mut paint = |x: i32, y: i32, w: i32, h: i32, color: Hsla| {
                if color.a <= 0.0 {
                    return;
                }
                window.paint_quad(fill(
                    Bounds {
                        origin: point(px(ox + x as f32 * px_size), px(oy + y as f32 * px_size)),
                        size: size(px(w as f32 * px_size), px(h as f32 * px_size)),
                    },
                    color,
                ));
            };

            let l: &Layers = match shape {
                Shape::Probe => &PROBE,
                Shape::Platter => &PLATTER,
                Shape::Person => &PERSON,
            };
            for (layer, color) in [
                (&l.a, tint.o),
                (&l.o, tint.o),
                (&l.f, tint.f),
                (&l.h, tint.h),
                (&l.s, tint.s),
                (&l.v, tint.v),
                (&l.d, tint.d),
            ] {
                for &(x, y, w) in layer {
                    paint(x, y, w, 1, dim(color, body_alpha));
                }
            }
            for &(x, y, w) in &l.led {
                paint(x, y, w, 1, dim(tint.led, if lamp_dim { 0.35 } else { 1.0 }));
            }
            let (le, re) = eyes(shape, mood);
            let dx = if shape == Shape::Probe && mood == Mood::Working {
                scan
            } else {
                0
            };
            for &(x, y, w, h) in le.iter().chain(re) {
                paint(x + dx, y, w, h, tint.eye);
            }
            for &(x, y, w, h) in sign(shape, mood) {
                paint(x, y, w, h, tint.sign);
            }
        },
    )
    .w(px(GW as f32 * px_size))
    .h(px(GH as f32 * px_size))
    .flex_shrink_0()
}

/// The probe as a menu-bar template image: RGBA, black with alpha, `scale` px per grid pixel and `pad` px of
/// clear margin. macOS tints a template for the menu bar, so only shape counts: the body is solid, the face
/// window is cut out, the crown keeps its dither (a machine's surface), and the eyes show the real state.
/// Returns `(rgba, width, height)`.
pub fn probe_template(mood: Mood, scan: i32, scale: u32, pad: (u32, u32)) -> (Vec<u8>, u32, u32) {
    let w = GW as u32 * scale + 2 * pad.0;
    let h = GH as u32 * scale + 2 * pad.1;
    let mut on = vec![false; (GW * GH) as usize];
    let mut put = |x: i32, y: i32, v: bool| {
        if (0..GW).contains(&x) && (0..GH).contains(&y) {
            on[(y * GW + x) as usize] = v;
        }
    };
    let l: &Layers = &PROBE;
    for layer in [&l.a, &l.o, &l.f, &l.h, &l.s, &l.led] {
        for &(x, y, run) in layer {
            for i in 0..run {
                put(x + i, y, true);
            }
        }
    }
    for layer in [&l.v, &l.d] {
        for &(x, y, run) in layer {
            for i in 0..run {
                put(x + i, y, false);
            }
        }
    }
    let (le, re) = eyes(Shape::Probe, mood);
    let dx = if mood == Mood::Working {
        scan.clamp(0, 6)
    } else {
        0
    };
    for &(x, y, ew, eh) in le.iter().chain(re) {
        for j in 0..eh {
            for i in 0..ew {
                put(x + dx + i, y + j, true);
            }
        }
    }

    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for gy in 0..GH {
        for gx in 0..GW {
            if !on[(gy * GW + gx) as usize] {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let px_x = pad.0 + gx as u32 * scale + sx;
                    let px_y = pad.1 + gy as u32 * scale + sy;
                    let i = ((px_y * w + px_x) * 4) as usize;
                    rgba[i + 3] = 255;
                }
            }
        }
    }
    (rgba, w, h)
}

// ---------------------------------------------------------------- dot-matrix figures

const ROWS: usize = 7;
const PITCH: f32 = 8.0;
const RADIUS: f32 = 3.0;

fn glyph(ch: char) -> &'static [&'static str] {
    match ch {
        '0' => &[
            "01110", "10001", "10011", "10101", "11001", "10001", "01110",
        ],
        '1' => &[
            "00100", "01100", "00100", "00100", "00100", "00100", "01110",
        ],
        '2' => &[
            "01110", "10001", "00001", "00010", "00100", "01000", "11111",
        ],
        '3' => &[
            "11110", "00001", "00001", "01110", "00001", "00001", "11110",
        ],
        '4' => &[
            "00010", "00110", "01010", "10010", "11111", "00010", "00010",
        ],
        '5' => &[
            "11111", "10000", "11110", "00001", "00001", "10001", "01110",
        ],
        '6' => &[
            "00110", "01000", "10000", "11110", "10001", "10001", "01110",
        ],
        '7' => &[
            "11111", "00001", "00010", "00100", "01000", "01000", "01000",
        ],
        '8' => &[
            "01110", "10001", "10001", "01110", "10001", "10001", "01110",
        ],
        '9' => &[
            "01110", "10001", "10001", "01111", "00001", "00010", "01100",
        ],
        '+' => &[
            "00000", "00100", "00100", "11111", "00100", "00100", "00000",
        ],
        // a decimal point, and a dash for a reading Samo doesn't have (never a made-up zero)
        '.' => &["0", "0", "0", "0", "0", "0", "1"],
        '-' => &["000", "000", "000", "111", "000", "000", "000"],
        _ => &["00", "00", "00", "00", "00", "01", "10"],
    }
}

/// Columns of dots a text takes, glyphs separated by one blank column.
pub fn dot_columns(text: &str) -> usize {
    let cols: usize = text.chars().map(|c| glyph(c)[0].len() + 1).sum();
    cols.saturating_sub(1).max(1)
}

/// Width in px of `text` drawn `height` px tall.
pub fn dot_width(text: &str, height: f32) -> f32 {
    dot_columns(text) as f32 * PITCH * height / (ROWS as f32 * PITCH)
}

/// A figure as round LED dots; unlit dots stay faintly there in `off`. For big figures only.
pub fn dot_number(text: &str, height: f32, on: Hsla, off: Hsla) -> Canvas<()> {
    let text = text.to_string();
    let scale = height / (ROWS as f32 * PITCH);
    let width = dot_width(&text, height);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let ox = f32::from(bounds.origin.x);
            let oy = f32::from(bounds.origin.y);
            let rr = RADIUS * scale;
            let mut col = 0usize;
            for ch in text.chars() {
                let g = glyph(ch);
                for (r, row) in g.iter().enumerate() {
                    for (c, bit) in row.bytes().enumerate() {
                        let cx = ((col + c) as f32 * PITCH + PITCH / 2.0) * scale;
                        let cy = (r as f32 * PITCH + PITCH / 2.0) * scale;
                        window.paint_quad(quad(
                            Bounds {
                                origin: point(px(ox + cx - rr), px(oy + cy - rr)),
                                size: size(px(rr * 2.0), px(rr * 2.0)),
                            },
                            px(rr),
                            if bit == b'1' { on } else { off },
                            px(0.0),
                            Hsla::transparent_black(),
                            Default::default(),
                        ));
                    }
                }
                col += g[0].len() + 1;
            }
        },
    )
    .w(px(width))
    .h(px(height))
    .flex_shrink_0()
}

// ---------------------------------------------------------------- meters

fn lit_count(ratio: Option<f32>, n: usize) -> usize {
    ratio
        .map(|r| {
            let r = r.clamp(0.0, 1.0);
            let k = (r * n as f32).round() as usize;
            if r > 0.0 { k.max(1) } else { 0 }
        })
        .unwrap_or(0)
}

/// A row of square LED segments. `None` lights nothing: unknown is not zero.
pub fn led_meter(ratio: Option<f32>, on: Hsla, off: Hsla, height: f32) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let (seg, gap) = (3.0_f32, 2.0_f32);
            let w = f32::from(bounds.size.width);
            let n = ((w + gap) / (seg + gap)).floor().max(1.0) as usize;
            let lit = lit_count(ratio, n);
            let ox = f32::from(bounds.origin.x).round();
            let oy = f32::from(bounds.origin.y).round();
            for i in 0..n {
                window.paint_quad(fill(
                    Bounds {
                        origin: point(px(ox + i as f32 * (seg + gap)), px(oy)),
                        size: size(px(seg), px(height)),
                    },
                    if i < lit { on } else { off },
                ));
            }
        },
    )
    .w_full()
    .h(px(height))
}

/// A vertical LED column, lit from the bottom.
pub fn led_column(ratio: Option<f32>, on: Hsla, off: Hsla, w: f32, h: f32) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let (seg, gap) = (3.0_f32, 2.0_f32);
            let n = ((h + gap) / (seg + gap)).floor().max(1.0) as usize;
            let lit = lit_count(ratio, n);
            let ox = f32::from(bounds.origin.x).round();
            let bottom = (f32::from(bounds.origin.y) + h).round();
            for i in 0..n {
                window.paint_quad(fill(
                    Bounds {
                        origin: point(px(ox), px(bottom - seg - i as f32 * (seg + gap))),
                        size: size(px(w), px(seg)),
                    },
                    if i < lit { on } else { off },
                ));
            }
        },
    )
    .w(px(w))
    .h(px(h))
    .flex_shrink_0()
}

/// A dashed vertical rule (2 px dashes every 4 px) between readout cells.
pub fn dashed_rule(color: Hsla) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let ox = f32::from(bounds.origin.x).round();
            let oy = f32::from(bounds.origin.y).round();
            let h = f32::from(bounds.size.height);
            let mut y = 0.0;
            while y < h {
                window.paint_quad(fill(
                    Bounds {
                        origin: point(px(ox), px(oy + y)),
                        size: size(px(1.0), px(2.0)),
                    },
                    color,
                ));
                y += 4.0;
            }
        },
    )
    .absolute()
    .left_0()
    .top(px(S3))
    .bottom(px(S3))
    .w(px(1.0))
}

// ---------------------------------------------------------------- the dither (Samo's suggestions)

/// Paints the 2 px checker inside `area`; with `band` = (radius, thickness) only a rounded-rectangle frame.
fn paint_checker(window: &mut Window, area: Bounds<Pixels>, band: Option<(f32, f32)>, color: Hsla) {
    let ox = f32::from(area.origin.x).round();
    let oy = f32::from(area.origin.y).round();
    let w = f32::from(area.size.width).floor();
    let h = f32::from(area.size.height).floor();
    let inside_round = |x: f32, y: f32, inset: f32, r: f32| -> bool {
        let (l, t, rr, b) = (inset, inset, w - inset, h - inset);
        if x < l || x > rr || y < t || y > b {
            return false;
        }
        let r = r.max(0.0);
        let cx = x.clamp(l + r, (rr - r).max(l + r));
        let cy = y.clamp(t + r, (b - r).max(t + r));
        let (dx, dy) = (x - cx, y - cy);
        dx * dx + dy * dy <= r * r
    };
    let cols = (w / 2.0) as i32;
    let rows = (h / 2.0) as i32;
    for j in 0..rows {
        let y = j as f32 * 2.0;
        let edge_row = band.is_some_and(|(_, t)| y < t || y >= h - t);
        for i in 0..cols {
            if (i + j) % 2 == 0 {
                continue;
            }
            let x = i as f32 * 2.0;
            if let Some((radius, thick)) = band {
                if !edge_row && x >= thick && x < w - thick {
                    continue;
                }
                let (mx, my) = (x + 1.0, y + 1.0);
                if !inside_round(mx, my, 0.0, radius) || inside_round(mx, my, thick, radius - thick)
                {
                    continue;
                }
            }
            window.paint_quad(fill(
                Bounds {
                    origin: point(px(ox + x), px(oy + y)),
                    size: size(px(2.0), px(2.0)),
                },
                color,
            ));
        }
    }
}

/// The dither frame over the edge of a surface Samo is only *suggesting*. Absolute and full-size, so the
/// surface keeps its size when the suggestion is acted on.
pub fn dither_frame(radius: f32, color: Hsla) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_checker(window, bounds, Some((radius, 4.0)), color),
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A block of dither (a legend swatch for "Samo suggests").
pub fn dither_block(w: f32, h: f32, color: Hsla) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_checker(window, bounds, None, color),
    )
    .w(px(w))
    .h(px(h))
    .flex_shrink_0()
}

/// A bar of two parts: what is (solid) and what Samo suggests clearing (dithered).
pub fn split_bar(solid: f32, suggested: f32, filled: f32, height: f32) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let pal = p();
            let total = (solid + suggested).max(f32::EPSILON);
            let w = (f32::from(bounds.size.width) * filled.clamp(0.0, 1.0))
                .max(4.0)
                .round();
            let solid_w = (w * solid / total).round();
            let ox = f32::from(bounds.origin.x).round();
            let oy = f32::from(bounds.origin.y).round();
            if solid_w > 0.0 {
                window.paint_quad(fill(
                    Bounds {
                        origin: point(px(ox), px(oy)),
                        size: size(px(solid_w), px(height)),
                    },
                    pal.text_2,
                ));
            }
            let gap = if solid_w > 0.0 && suggested > 0.0 {
                2.0
            } else {
                0.0
            };
            let rest = w - solid_w - gap;
            if suggested > 0.0 && rest > 0.0 {
                paint_checker(
                    window,
                    Bounds {
                        origin: point(px(ox + solid_w + gap), px(oy)),
                        size: size(px(rest), px(height)),
                    },
                    None,
                    pal.accent,
                );
            }
        },
    )
    .w_full()
    .h(px(height))
}

// ---------------------------------------------------------------- the ring (Samo's shape)

#[derive(Clone, Copy)]
pub enum ArcFill {
    Solid(Hsla),
    /// A suggestion: Cobalt checker, the same grammar as the dither frame.
    Dither(Hsla),
}

#[derive(Clone, Copy)]
pub struct RingPart {
    pub share: f32,
    pub fill: ArcFill,
}

fn sector(cx: f32, cy: f32, r_in: f32, r_out: f32, a0: f32, a1: f32) -> Vec<Point<Pixels>> {
    let steps = (((a1 - a0).abs() / (2.0_f32.to_radians())).ceil() as usize).max(2);
    let mut pts = Vec::with_capacity(steps * 2 + 2);
    for i in 0..=steps {
        let a = a0 + (a1 - a0) * i as f32 / steps as f32;
        pts.push(point(px(cx + r_out * a.cos()), px(cy + r_out * a.sin())));
    }
    for i in (0..=steps).rev() {
        let a = a0 + (a1 - a0) * i as f32 / steps as f32;
        pts.push(point(px(cx + r_in * a.cos()), px(cy + r_in * a.sin())));
    }
    pts
}

/// The disk gauge: a ring of parts going clockwise from twelve o'clock over a faint track, with a bezel of
/// twelve ticks. A 2-degree gap of surface separates the parts.
pub fn ring_gauge(
    diameter: f32,
    thickness: f32,
    parts: Vec<RingPart>,
    track: Hsla,
    ticks: Hsla,
) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let cx = f32::from(bounds.origin.x) + diameter / 2.0;
            let cy = f32::from(bounds.origin.y) + diameter / 2.0;
            let r_out = diameter / 2.0 - 8.0;
            let r_in = r_out - thickness;

            let mut bezel = PathBuilder::stroke(px(1.5));
            for i in 0..12 {
                let a = i as f32 / 12.0 * TAU - PI / 2.0;
                bezel.move_to(point(
                    px(cx + (r_out + 3.0) * a.cos()),
                    px(cy + (r_out + 3.0) * a.sin()),
                ));
                bezel.line_to(point(
                    px(cx + (r_out + 7.0) * a.cos()),
                    px(cy + (r_out + 7.0) * a.sin()),
                ));
            }
            if let Ok(path) = bezel.build() {
                window.paint_path(path, ticks);
            }

            let mut tr = PathBuilder::fill();
            tr.add_polygon(&sector(cx, cy, r_in, r_out, -PI / 2.0, 1.5 * PI), true);
            if let Ok(path) = tr.build() {
                window.paint_path(path, track);
            }

            let visible = parts.iter().filter(|p| p.share > 0.0005).count();
            let gap = if visible > 1 {
                2.0_f32.to_radians()
            } else {
                0.0
            };
            let mut start = -PI / 2.0;
            for part in parts.iter().filter(|p| p.share > 0.0005) {
                let sweep = part.share.clamp(0.0, 1.0) * TAU;
                let (a0, a1) = (start, start + (sweep - gap).max(0.6_f32.to_radians()));
                start += sweep;
                match part.fill {
                    ArcFill::Solid(color) => {
                        let mut b = PathBuilder::fill();
                        b.add_polygon(&sector(cx, cy, r_in, r_out, a0, a1), true);
                        if let Ok(path) = b.build() {
                            window.paint_path(path, color);
                        }
                    }
                    ArcFill::Dither(color) => {
                        let ox = (cx - r_out).floor();
                        let oy = (cy - r_out).floor();
                        let n = r_out.ceil() as i32 + 1;
                        for j in 0..n {
                            for i in 0..n {
                                if (i + j) % 2 == 0 {
                                    continue;
                                }
                                let (x, y) = (ox + i as f32 * 2.0, oy + j as f32 * 2.0);
                                let (dx, dy) = (x + 1.0 - cx, y + 1.0 - cy);
                                let r = (dx * dx + dy * dy).sqrt();
                                if r < r_in || r > r_out {
                                    continue;
                                }
                                let mut a = dy.atan2(dx);
                                while a < a0 {
                                    a += TAU;
                                }
                                if a > a1 {
                                    continue;
                                }
                                window.paint_quad(fill(
                                    Bounds {
                                        origin: point(px(x), px(y)),
                                        size: size(px(2.0), px(2.0)),
                                    },
                                    color,
                                ));
                            }
                        }
                    }
                }
            }
        },
    )
    .w(px(diameter))
    .h(px(diameter))
    .flex_shrink_0()
}

/// The page's dial: its icon set in a ring with a bezel of twelve ticks (Samo's answer to Mesh's hexagon).
pub fn dial_chip(icon_name: &str, d: f32) -> Div {
    let pal = p();
    let ticks = pal.accent_line;
    div()
        .relative()
        .size(px(d))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(pal.accent_wash)
        .border_1()
        .border_color(pal.accent_line)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let c = f32::from(bounds.size.width) / 2.0;
                    let cx = f32::from(bounds.origin.x) + c;
                    let cy = f32::from(bounds.origin.y) + c;
                    let mut b = PathBuilder::stroke(px(1.25));
                    for i in 0..12 {
                        let a = i as f32 / 12.0 * TAU;
                        let (r0, r1) = (c - 5.5, c - 2.5);
                        b.move_to(point(px(cx + r0 * a.cos()), px(cy + r0 * a.sin())));
                        b.line_to(point(px(cx + r1 * a.cos()), px(cy + r1 * a.sin())));
                    }
                    if let Ok(path) = b.build() {
                        window.paint_path(path, ticks);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(crate::components::icon(
            icon_name,
            d * 0.42,
            pal.accent_text,
        ))
}

/// Samo's mark: a gauge — a ring open at the bottom, a needle and a hub — on an Ink disc.
pub fn logo_mark(d: f32) -> Div {
    let pal = p();
    let ink = pal.primary_fg;
    div()
        .size(px(d))
        .flex_shrink_0()
        .rounded_full()
        .bg(pal.primary_bg)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let c = f32::from(bounds.size.width) / 2.0;
                    let cx = f32::from(bounds.origin.x) + c;
                    let cy = f32::from(bounds.origin.y) + c + c * 0.06;
                    let r = c * 0.56;
                    let mut arc = PathBuilder::stroke(px((d / 14.0).max(2.0)));
                    let (a0, a1) = (0.75 * PI, 2.25 * PI);
                    for i in 0..=36 {
                        let a = a0 + (a1 - a0) * i as f32 / 36.0;
                        let p = point(px(cx + r * a.cos()), px(cy + r * a.sin()));
                        if i == 0 {
                            arc.move_to(p);
                        } else {
                            arc.line_to(p);
                        }
                    }
                    let needle = -0.28 * PI;
                    arc.move_to(point(px(cx), px(cy)));
                    arc.line_to(point(
                        px(cx + r * 0.8 * needle.cos()),
                        px(cy + r * 0.8 * needle.sin()),
                    ));
                    if let Ok(path) = arc.build() {
                        window.paint_path(path, ink);
                    }
                    let hub = (d / 9.0).max(3.0);
                    window.paint_quad(quad(
                        Bounds {
                            origin: point(px(cx - hub / 2.0), px(cy - hub / 2.0)),
                            size: size(px(hub), px(hub)),
                        },
                        px(hub / 2.0),
                        ink,
                        px(0.0),
                        Hsla::transparent_black(),
                        Default::default(),
                    ));
                },
            )
            .size_full(),
        )
}

/// The measuring grid along the top of the stage: dots on a 16 px square lattice, every fourth a little
/// stronger like a ruler's major ticks, fading out downward over `height`.
pub fn measure_grid(height: f32) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let dot = p().dot;
            let step = 16.0_f32;
            let w = f32::from(bounds.size.width);
            let ox = f32::from(bounds.origin.x).round();
            let oy = f32::from(bounds.origin.y).round();
            let rows = (height / step) as i32;
            let cols = (w / step) as i32;
            for j in 1..rows {
                let fade = 1.0 - (j as f32 * step) / height;
                for i in 1..cols {
                    let major = i % 4 == 0 && j % 4 == 0;
                    let mut c = dot;
                    c.a *= fade * if major { 1.8 } else { 1.0 };
                    let s = if major { 2.0 } else { 1.5 };
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(
                                px(ox + i as f32 * step - s / 2.0),
                                px(oy + j as f32 * step - s / 2.0),
                            ),
                            size: size(px(s), px(s)),
                        },
                        c,
                    ));
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h(px(height))
}

// ---------------------------------------------------------------- text

/// One line of text that ends in "…" when it doesn't fit, truncated at paint time against the final width.
/// It has no width of its own: give it `.flex_1()` in a row or `.w_full()` in a column.
pub fn clip_text(
    text: impl Into<SharedString>,
    font: Font,
    font_size: f32,
    color: Hsla,
) -> Canvas<()> {
    clip_text_aligned(text, font, font_size, color, false)
}

/// `clip_text`, optionally centred in its box.
pub fn clip_text_aligned(
    text: impl Into<SharedString>,
    font: Font,
    font_size: f32,
    color: Hsla,
    center: bool,
) -> Canvas<()> {
    let text: SharedString = text.into();
    let line_h = (font_size * 1.4).round();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            if text.is_empty() {
                return;
            }
            let size_px = px(font_size);
            let mut runs = vec![TextRun {
                len: text.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            let shown = cx
                .text_system()
                .line_wrapper(font.clone(), size_px)
                .truncate_line(text.clone(), bounds.size.width, "…", &mut runs);
            let shaped = window.text_system().shape_line(shown, size_px, &runs, None);
            let y = bounds.origin.y + (bounds.size.height - px(line_h)) / 2.0;
            let x = if center {
                bounds.origin.x + ((bounds.size.width - shaped.width) / 2.0).max(px(0.0))
            } else {
                bounds.origin.x
            };
            let _ = shaped.paint(point(x, y), px(line_h), window, cx);
        },
    )
    .h(px(line_h))
    .min_w(px(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(r: &Runs) -> i32 {
        r.iter().map(|x| x.2).sum()
    }

    #[test]
    fn probe_is_round_with_feet_and_antenna() {
        let g = body(4, &ROUND_13);
        let widest = (0..GH)
            .map(|y| (0..GW).filter(|x| at(&g, *x, y)).count())
            .max();
        assert_eq!(widest, Some(14));
        // antenna lamp on top, centred; two feet at the bottom row
        assert_eq!(PROBE.led, vec![(7, 0, 2)]);
        assert!(PROBE.a.contains(&(4, 17, 2)) && PROBE.a.contains(&(10, 17, 2)));
    }

    #[test]
    fn layers_partition_the_body() {
        let g = body(4, &ROUND_13);
        let pixels = (0..GH)
            .map(|y| (0..GW).filter(|x| at(&g, *x, y)).count() as i32)
            .sum::<i32>();
        assert_eq!(
            count(&PROBE.o) + count(&PROBE.f) + count(&PROBE.h) + count(&PROBE.s),
            pixels
        );
    }

    #[test]
    fn probe_face_has_cut_corners_and_a_dithered_crown() {
        assert_eq!(count(&PROBE.v), 10 * 5 - 4);
        assert!(!PROBE.d.is_empty());
        assert!(PROBE.d.iter().all(|&(_, y, _)| (5..8).contains(&y)));
    }

    #[test]
    fn working_scan_bar_stays_inside_the_face() {
        let (bar, _) = eyes(Shape::Probe, Mood::Working);
        for &(x, _, w, _) in bar {
            assert!(x >= 4 && x + w + 6 <= 12);
        }
    }

    #[test]
    fn dot_figures_measure_their_glyphs() {
        assert_eq!(dot_columns("8"), 5);
        assert_eq!(dot_columns("12"), 11);
        assert_eq!(dot_columns("1.5"), 5 + 1 + 1 + 1 + 5);
        assert!((dot_width("8", 56.0) - 40.0).abs() < 0.01);
    }

    #[test]
    fn menu_bar_probe_is_pixel_exact_with_a_cut_out_face() {
        let (rgba, w, h) = probe_template(Mood::Idle, 0, 2, (2, 4));
        assert_eq!((w, h), (36, 44));
        assert_eq!(rgba.len(), (36 * 44 * 4) as usize);
        let alpha = |gx: u32, gy: u32| rgba[(((4 + gy * 2) * w + 2 + gx * 2) * 4 + 3) as usize];
        // body solid, face window clear, eyes solid inside it, margin clear
        assert_eq!(alpha(7, 14), 255);
        assert_eq!(alpha(4, 9), 0);
        assert_eq!(alpha(5, 9), 255);
        assert_eq!(rgba[3], 0);
    }

    #[test]
    fn menu_bar_scan_bar_moves_with_the_step() {
        let (a, w, _) = probe_template(Mood::Working, 0, 1, (0, 0));
        let (b, _, _) = probe_template(Mood::Working, 4, 1, (0, 0));
        let at = |img: &[u8], x: u32, y: u32| img[((y * w + x) * 4 + 3) as usize];
        assert_eq!(at(&a, 4, 9), 255);
        assert_eq!(at(&b, 4, 9), 0);
        assert_eq!(at(&b, 8, 9), 255);
    }

    #[test]
    fn meters_never_light_an_unknown() {
        assert_eq!(lit_count(None, 20), 0);
        assert_eq!(lit_count(Some(0.0), 20), 0);
        assert_eq!(lit_count(Some(0.001), 20), 1);
        assert_eq!(lit_count(Some(1.0), 20), 20);
    }
}
