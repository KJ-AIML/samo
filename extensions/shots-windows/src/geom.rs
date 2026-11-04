//! Points and rectangles for the overlay, the editor and their maths: f64, origin top left (Windows'), with Core
//! Graphics' rules where the Mac's code relied on them (`intersection`, `standardized`, `integral`).

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// The same rectangle with a positive width and height.
    pub fn standardized(&self) -> Rect {
        let (x, w) = if self.w < 0.0 {
            (self.x + self.w, -self.w)
        } else {
            (self.x, self.w)
        };
        let (y, h) = if self.h < 0.0 {
            (self.y + self.h, -self.h)
        } else {
            (self.y, self.h)
        };
        Rect::new(x, y, w, h)
    }

    pub fn min_x(&self) -> f64 {
        self.standardized().x
    }

    pub fn min_y(&self) -> f64 {
        self.standardized().y
    }

    pub fn max_x(&self) -> f64 {
        let s = self.standardized();
        s.x + s.w
    }

    pub fn max_y(&self) -> f64 {
        let s = self.standardized();
        s.y + s.h
    }

    /// Where the two overlap; `None` if they don't.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x = self.min_x().max(other.min_x());
        let y = self.min_y().max(other.min_y());
        let right = self.max_x().min(other.max_x());
        let bottom = self.max_y().min(other.max_y());
        (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
    }

    /// The smallest rectangle of whole units that covers it.
    pub fn integral(&self) -> Rect {
        let x = self.min_x().floor();
        let y = self.min_y().floor();
        Rect::new(x, y, self.max_x().ceil() - x, self.max_y().ceil() - y)
    }

    /// Whether `p` is inside: the left and top edges are, the right and bottom aren't.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min_x() && p.x < self.max_x() && p.y >= self.min_y() && p.y < self.max_y()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_meet_like_core_graphics() {
        let a = Rect::new(0.0, 0.0, 30.0, 30.0);
        let b = Rect::new(-10.0, -10.0, 50.0, 50.0);
        assert_eq!(b.intersection(&a), Some(Rect::new(0.0, 0.0, 30.0, 30.0)));
        assert_eq!(a.intersection(&Rect::new(40.0, 40.0, 5.0, 5.0)), None);
        // A negative size is turned round.
        assert_eq!(
            Rect::new(10.0, 10.0, -4.0, -6.0).standardized(),
            Rect::new(6.0, 4.0, 4.0, 6.0)
        );
        // Whole pixels, growing to cover: 50.4,20.2 200×150 is 50,20 201×151.
        assert_eq!(
            Rect::new(50.4, 20.2, 200.0, 150.0).integral(),
            Rect::new(50.0, 20.0, 201.0, 151.0)
        );
        assert!(a.contains(Point::new(29.9, 0.0)) && !a.contains(Point::new(30.0, 0.0)));
        assert_eq!((a.max_x(), a.max_y()), (30.0, 30.0));
    }
}
