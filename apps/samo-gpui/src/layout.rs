//! Responsive layout, decided from the window's size every frame. One set of breakpoints for the whole app:
//! the sidebar folds into an icon rail, two-column pages stack, readouts wrap, and table columns step away.

use crate::theme::{RAIL_W, SIDEBAR_W};

/// The stage is set in from the frame by this much (top, right, bottom).
pub const STAGE_INSET: f32 = 8.0;
/// Content never runs wider than this; extra width becomes margin.
pub const CONTENT_MAX: f32 = 1320.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub window_w: f32,
    /// The sidebar is an icon rail.
    pub rail: bool,
    pub sidebar_w: f32,
    /// Horizontal padding inside the stage.
    pub pad: f32,
    /// Width of the content column.
    pub content_w: f32,
}

impl Layout {
    pub fn for_window(window_w: f32) -> Self {
        let rail = window_w < 1100.0;
        let sidebar_w = if rail { RAIL_W } else { SIDEBAR_W };
        let stage_w = (window_w - sidebar_w - STAGE_INSET).max(0.0);
        let pad = (stage_w * 0.035)
            .clamp(16.0, 40.0)
            .max((stage_w - CONTENT_MAX) / 2.0);
        Self {
            window_w,
            rail,
            sidebar_w,
            pad,
            content_w: (stage_w - 2.0 * pad).max(0.0),
        }
    }

    /// Two columns side by side.
    pub fn wide(&self) -> bool {
        self.content_w >= 940.0
    }

    /// Four readouts in one row; tables keep every column.
    pub fn roomy(&self) -> bool {
        self.content_w >= 700.0
    }

    /// The side column of a two-column page.
    pub fn side_w(&self) -> f32 {
        if self.content_w >= 1120.0 {
            340.0
        } else {
            300.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_windows_get_a_rail_and_one_column() {
        let l = Layout::for_window(760.0);
        assert!(l.rail && !l.wide());
        assert!(l.content_w > 560.0);
    }

    #[test]
    fn big_windows_get_the_sidebar_and_two_columns() {
        let l = Layout::for_window(1440.0);
        assert!(!l.rail && l.wide() && l.roomy());
    }

    #[test]
    fn content_is_held_to_a_readable_width() {
        let l = Layout::for_window(3000.0);
        assert!((l.content_w - CONTENT_MAX).abs() < 1.0);
    }
}
