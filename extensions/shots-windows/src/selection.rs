//! Selection maths for the capture overlay, in screen coordinates: pixels from the primary monitor's top left, as
//! Windows gives them (a monitor to its left or above has negative ones). A monitor's captured image is its own
//! pixels from its top left. Ported from the Mac's `Selection.swift`, without its flip.

use crate::geom::{Point, Rect};

/// Below this many pixels across, a drag is a click: nothing is captured.
pub const MINIMUM: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

/// The rectangle from where the drag began to where it is now. With `square`, the shorter side wins, growing from
/// the start in the drag's direction.
pub fn rect(a: Point, b: Point, square: bool) -> Rect {
    let (mut dx, mut dy) = (b.x - a.x, b.y - a.y);
    if square {
        let side = dx.abs().min(dy.abs());
        dx = if dx < 0.0 { -side } else { side };
        dy = if dy < 0.0 { -side } else { side };
    }
    Rect::new(a.x.min(a.x + dx), a.y.min(a.y + dy), dx.abs(), dy.abs())
}

/// `r` kept within `bounds`; `None` if nothing of it is.
pub fn clamp(r: Rect, bounds: Rect) -> Option<Rect> {
    r.intersection(&bounds)
}

/// `r` (screen pixels) as a rectangle of the image of the monitor whose top left is `origin`, captured at `scale`
/// times its pixels: whole pixels.
pub fn image_rect(r: Rect, origin: Point, scale: f64) -> Rect {
    Rect::new(
        ((r.x - origin.x) * scale).round(),
        ((r.y - origin.y) * scale).round(),
        (r.w * scale).round(),
        (r.h * scale).round(),
    )
}

/// The pointer moved by an arrow key: one pixel, or ten with Shift.
pub fn nudge(p: Point, arrow: Arrow, big: bool) -> Point {
    let step = if big { 10.0 } else { 1.0 };
    match arrow {
        Arrow::Up => Point::new(p.x, p.y - step),
        Arrow::Down => Point::new(p.x, p.y + step),
        Arrow::Left => Point::new(p.x - step, p.y),
        Arrow::Right => Point::new(p.x + step, p.y),
    }
}

/// `v` as a whole index below `len`: rounded down, kept inside.
fn index(v: f64, len: usize) -> usize {
    let top = len.saturating_sub(1);
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    // A float-to-integer `as` saturates; the result is kept inside the image.
    (v.floor() as usize).min(top)
}

/// The pixel of the monitor's image (`size`, rows from the top) under screen point `p`, clamped to the image.
pub fn pixel(p: Point, origin: Point, scale: f64, size: (usize, usize)) -> (usize, usize) {
    (
        index((p.x - origin.x) * scale, size.0),
        index((p.y - origin.y) * scale, size.1),
    )
}

/// `r` moved by `delta`, kept whole inside `bounds` (Space held while dragging moves the selection).
pub fn moved(r: Rect, delta: Point, bounds: Rect) -> Rect {
    let x = (r.x + delta.x)
        .min(bounds.max_x() - r.w)
        .max(bounds.min_x());
    let y = (r.y + delta.y)
        .min(bounds.max_y() - r.h)
        .max(bounds.min_y());
    Rect::new(x, y, r.w, r.h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_maths() {
        // Drags in any direction give the same rectangle.
        assert_eq!(
            rect(Point::new(100.0, 80.0), Point::new(40.0, 20.0), false),
            Rect::new(40.0, 20.0, 60.0, 60.0)
        );
        assert_eq!(
            rect(Point::new(10.0, 10.0), Point::new(70.0, 30.0), false),
            Rect::new(10.0, 10.0, 60.0, 20.0)
        );
        // Shift keeps a square, growing from the start the way the drag went.
        assert_eq!(
            rect(Point::new(100.0, 100.0), Point::new(40.0, 80.0), true),
            Rect::new(80.0, 80.0, 20.0, 20.0)
        );
        // Within the monitor.
        assert_eq!(
            clamp(
                Rect::new(-10.0, -10.0, 50.0, 50.0),
                Rect::new(0.0, 0.0, 30.0, 30.0)
            ),
            Some(Rect::new(0.0, 0.0, 30.0, 30.0))
        );
        // Arrows nudge by one, Shift by ten; down is down.
        assert_eq!(
            nudge(Point::new(5.0, 5.0), Arrow::Right, false),
            Point::new(6.0, 5.0)
        );
        assert_eq!(
            nudge(Point::new(5.0, 5.0), Arrow::Down, true),
            Point::new(5.0, 15.0)
        );
        // Moving a selection keeps it whole on the monitor.
        assert_eq!(
            moved(
                Rect::new(10.0, 10.0, 20.0, 20.0),
                Point::new(100.0, -50.0),
                Rect::new(0.0, 0.0, 100.0, 100.0)
            ),
            Rect::new(80.0, 0.0, 20.0, 20.0)
        );
    }

    /// Windows' screen is in pixels from the primary monitor's top left; a monitor's image is its own pixels from its
    /// top left, so a monitor to the left (negative x) or above works the same.
    #[test]
    fn screen_points_to_a_monitors_pixels() {
        let left = Point::new(-1920.0, -200.0);
        assert_eq!(
            image_rect(Rect::new(-1910.0, -100.0, 100.0, 50.0), left, 1.0),
            Rect::new(10.0, 100.0, 100.0, 50.0)
        );
        // At twice the pixels (a capture kept at another scale).
        assert_eq!(
            image_rect(
                Rect::new(10.0, 100.0, 100.0, 50.0),
                Point::new(0.0, 0.0),
                2.0
            ),
            Rect::new(20.0, 200.0, 200.0, 100.0)
        );
        let size = (400, 200);
        assert_eq!(
            pixel(Point::new(10.4, 5.6), Point::new(0.0, 0.0), 2.0, size),
            (20, 11)
        );
        assert_eq!(pixel(Point::new(-1915.2, -190.0), left, 1.0, size), (4, 10));
        // Never outside the image.
        assert_eq!(
            pixel(Point::new(500.0, -3.0), Point::new(0.0, 0.0), 2.0, size),
            (399, 0)
        );
    }
}
