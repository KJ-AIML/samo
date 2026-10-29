//! GPU-painted charts. Gaps (`None`) break the line instead of being interpolated (R3).

use crate::theme::*;
use gpui::{
    Bounds, Canvas, Hsla, PathBuilder, Pixels, Point, Styled, canvas, fill, linear_color_stop,
    linear_gradient, point, px, size,
};
use std::cell::Cell;
use std::rc::Rc;

/// Where a chart last painted, shared with its pointer handler so a position can become a sample.
pub type ChartBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

/// One line's samples, oldest first, as `(slot, value)`: slot `0` is the left edge and `capacity - 1` the right.
/// A `None` value is a stopped sensor and breaks the line; a slot with no entry is skipped over.
pub type Samples = Vec<(usize, Option<f32>)>;

pub struct LineChart {
    /// One or more lines on one scale, each with its colour. The first also gets a wash beneath it.
    pub lines: Vec<(Samples, Hsla)>,
    /// How many slots the full width holds.
    pub capacity: usize,
    /// The value at the top edge (100 for a percentage).
    pub max: f32,
    pub gridlines: bool,
    /// A slot to mark with a crosshair and a dot on each line.
    pub mark: Option<usize>,
    /// Filled in on every paint, for the pointer handler.
    pub bounds: Option<ChartBounds>,
}

/// The last `capacity` values as slots, newest at the right edge.
pub fn by_index(values: &[Option<f32>], capacity: usize) -> Samples {
    let start = values.len().saturating_sub(capacity);
    let visible = &values[start..];
    let offset = capacity.saturating_sub(visible.len());
    visible
        .iter()
        .enumerate()
        .map(|(i, v)| (offset + i, *v))
        .collect()
}

/// The slot under `x`, for a chart `width` wide starting at `left`.
pub fn slot_at(x: f32, left: f32, width: f32, capacity: usize) -> usize {
    if width <= 0.0 || capacity < 2 {
        return capacity.saturating_sub(1);
    }
    let frac = ((x - left) / width).clamp(0.0, 1.0);
    (frac * (capacity - 1) as f32).round() as usize
}

/// Where a slot sits across a chart `width` wide.
pub fn slot_x(slot: usize, width: f32, capacity: usize) -> f32 {
    if capacity < 2 {
        return width;
    }
    width * slot.min(capacity - 1) as f32 / (capacity - 1) as f32
}

impl LineChart {
    /// One percentage line from the last `capacity` samples, no crosshair.
    pub fn percent(values: &[Option<f32>], capacity: usize, color: Hsla, gridlines: bool) -> Self {
        Self {
            lines: vec![(by_index(values, capacity), color)],
            capacity,
            max: 100.0,
            gridlines,
            mark: None,
            bounds: None,
        }
    }

    pub fn render(self) -> Canvas<()> {
        let LineChart {
            lines,
            capacity,
            max,
            gridlines,
            mark,
            bounds: sink,
        } = self;
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if let Some(sink) = &sink {
                    sink.set(Some(bounds));
                }
                let pal = p();
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                if w <= 4.0 || h <= 4.0 {
                    return;
                }
                let ox = f32::from(bounds.origin.x);
                let oy = f32::from(bounds.origin.y);

                if gridlines {
                    for frac in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
                        let y = (oy + (h - 1.0) * frac).round();
                        window.paint_quad(fill(
                            Bounds {
                                origin: point(px(ox), px(y)),
                                size: size(px(w), px(1.0)),
                            },
                            pal.hairline,
                        ));
                    }
                }

                let cap = capacity.max(2);
                let top = if max > 0.0 { max } else { 1.0 };
                let inset = 2.0;
                let to_point = |slot: usize, v: f32| -> Point<Pixels> {
                    let x = ox + slot_x(slot, w, cap);
                    let y = oy + inset + (h - inset * 2.0) * (1.0 - (v / top).clamp(0.0, 1.0));
                    point(px(x), px(y))
                };
                let baseline = oy + h;

                for (n, (samples, color)) in lines.iter().enumerate() {
                    // Split into contiguous runs so gaps stay visible.
                    let mut runs: Vec<Vec<Point<Pixels>>> = Vec::new();
                    let mut current: Vec<Point<Pixels>> = Vec::new();
                    for (slot, v) in samples.iter().filter(|(s, _)| *s < cap) {
                        match v {
                            Some(v) => current.push(to_point(*slot, *v)),
                            None => {
                                if current.len() > 1 {
                                    runs.push(std::mem::take(&mut current));
                                } else {
                                    current.clear();
                                }
                            }
                        }
                    }
                    if current.len() > 1 {
                        runs.push(current);
                    }

                    for run in &runs {
                        if n == 0 {
                            let mut wash_top = *color;
                            wash_top.a = 0.16;
                            let mut wash_bottom = *color;
                            wash_bottom.a = 0.0;
                            let mut area = PathBuilder::fill();
                            if let (Some(first), Some(last)) = (run.first(), run.last()) {
                                area.move_to(point(first.x, px(baseline)));
                                for p in run {
                                    area.line_to(*p);
                                }
                                area.line_to(point(last.x, px(baseline)));
                                area.close();
                            }
                            if let Ok(path) = area.build() {
                                window.paint_path(
                                    path,
                                    linear_gradient(
                                        180.0,
                                        linear_color_stop(wash_top, 0.0),
                                        linear_color_stop(wash_bottom, 1.0),
                                    ),
                                );
                            }
                        }

                        let mut stroke = PathBuilder::stroke(px(2.0));
                        for (i, p) in run.iter().enumerate() {
                            if i == 0 {
                                stroke.move_to(*p);
                            } else {
                                stroke.line_to(*p);
                            }
                        }
                        if let Ok(path) = stroke.build() {
                            window.paint_path(path, *color);
                        }
                    }
                }

                // The crosshair: a hairline down the chart and an 8 px dot, ringed in the card, on each line.
                if let Some(slot) = mark.filter(|s| *s < cap) {
                    let x = (ox + slot_x(slot, w, cap)).round();
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(px(x), px(oy)),
                            size: size(px(1.0), px(h)),
                        },
                        pal.hairline_strong,
                    ));
                    for (samples, color) in &lines {
                        let value = samples.iter().find(|(s, _)| *s == slot).and_then(|s| s.1);
                        if let Some(v) = value {
                            let c = to_point(slot, v);
                            for (d, paint) in [(12.0_f32, pal.card), (8.0, *color)] {
                                window.paint_quad(
                                    fill(
                                        Bounds {
                                            origin: point(c.x - px(d / 2.0), c.y - px(d / 2.0)),
                                            size: size(px(d), px(d)),
                                        },
                                        paint,
                                    )
                                    .corner_radii(px(d / 2.0)),
                                );
                            }
                        }
                    }
                }
            },
        )
        .size_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_values_sit_at_the_right_edge() {
        let values = [Some(1.0), None, Some(3.0)];
        assert_eq!(
            by_index(&values, 5),
            vec![(2, Some(1.0)), (3, None), (4, Some(3.0))]
        );
        // More values than room: only the newest are kept.
        assert_eq!(by_index(&values, 2), vec![(0, None), (1, Some(3.0))]);
    }

    #[test]
    fn a_pointer_position_becomes_a_slot_and_back() {
        assert_eq!(slot_at(100.0, 100.0, 300.0, 121), 0);
        assert_eq!(slot_at(400.0, 100.0, 300.0, 121), 120);
        assert_eq!(slot_at(250.0, 100.0, 300.0, 121), 60);
        // Outside the chart it clamps to an edge.
        assert_eq!(slot_at(900.0, 100.0, 300.0, 121), 120);
        assert_eq!(slot_x(60, 300.0, 121), 150.0);
    }
}
