//! Text read from a shot, put in reading order (ADR-004 S4): rows top to bottom, the pieces of one row left to right.
//! Boxes are the OCR's, origin top left. Ported from the Mac's `TextReader.lines`.

use crate::geom::Rect;

/// A piece of text the OCR found, with where.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundText {
    pub text: String,
    pub bounds: Rect,
}

/// Lines in reading order: top to bottom, and the pieces of one line left to right, joined by a space. Two pieces are
/// on one line when they overlap by at least half the smaller one's height. Blank lines are dropped.
pub fn lines(found: &[FoundText]) -> Vec<String> {
    let mut pieces: Vec<&FoundText> = found.iter().collect();
    let mid = |f: &FoundText| f.bounds.min_y() + f.bounds.standardized().h / 2.0;
    pieces.sort_by(|a, b| mid(a).total_cmp(&mid(b)));
    let mut rows: Vec<Vec<&FoundText>> = Vec::new();
    for piece in pieces {
        if let Some(row) = rows.last_mut()
            && let Some(last) = row.last()
        {
            let overlap = last.bounds.max_y().min(piece.bounds.max_y())
                - last.bounds.min_y().max(piece.bounds.min_y());
            let smaller = last
                .bounds
                .standardized()
                .h
                .min(piece.bounds.standardized().h);
            if overlap >= 0.5 * smaller {
                row.push(piece);
                continue;
            }
        }
        rows.push(vec![piece]);
    }
    rows.into_iter()
        .map(|mut row| {
            row.sort_by(|a, b| a.bounds.min_x().total_cmp(&b.bounds.min_x()));
            row.iter()
                .map(|f| f.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|line| !line.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, x: f64, y: f64, w: f64, h: f64) -> FoundText {
        FoundText {
            text: text.into(),
            bounds: Rect::new(x, y, w, h),
        }
    }

    #[test]
    fn lines_in_reading_order() {
        // Two pieces of one line, found right first; and the lower line found first.
        let pieces = [
            found("second line", 10.0, 70.0, 50.0, 10.0),
            found("world", 50.0, 28.0, 20.0, 10.0),
            found("สวัสดี", 10.0, 30.0, 30.0, 10.0),
        ];
        assert_eq!(lines(&pieces), ["สวัสดี world", "second line"]);
        assert!(lines(&[]).is_empty());
        assert!(
            lines(&[found("  ", 0.0, 0.0, 5.0, 5.0)]).is_empty(),
            "blank lines are dropped"
        );
    }
}
