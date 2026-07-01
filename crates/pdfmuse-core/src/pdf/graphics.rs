//! Vector-graphics collection helpers.
//!
//! The content interpreter (`content.rs`) feeds device-space path points here;
//! these helpers flip Y to the IR's top-left origin, build [`Rect`] / [`Rule`],
//! and drop degenerate segments. The collected geometry is the source of table
//! borders for ruled-table reconstruction (PER-43).

use crate::ir::{BBox, Rect, Rule};

/// Minimum length (pt) for a rule segment to be worth keeping.
const MIN_LEN: f32 = 0.5;

/// Build a [`Rule`] from two device-space points (Y flipped to top-left origin).
/// Returns `None` for degenerate near-zero-length segments.
pub(super) fn make_rule(p0: (f32, f32), p1: (f32, f32), width: f32, page_height: f32) -> Option<Rule> {
    let (x0, y0) = (p0.0, page_height - p0.1);
    let (x1, y1) = (p1.0, page_height - p1.1);
    if (x1 - x0).hypot(y1 - y0) < MIN_LEN {
        return None;
    }
    Some(Rule { x0, y0, x1, y1, width })
}

/// Build an axis-aligned [`Rect`] from four device-space corners (Y flipped).
pub(super) fn make_rect(corners: [(f32, f32); 4], page_height: f32) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, yd) in corners {
        let y = page_height - yd;
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    Rect { bbox: BBox { x0, y0, x1, y1 } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degenerate_rule_is_dropped() {
        assert!(make_rule((10.0, 10.0), (10.0, 10.2), 1.0, 100.0).is_none());
        assert!(make_rule((10.0, 10.0), (60.0, 10.0), 1.0, 100.0).is_some());
    }

    #[test]
    fn rule_flips_y_to_top_left() {
        let r = make_rule((0.0, 90.0), (50.0, 90.0), 1.0, 100.0).unwrap();
        // Device y=90 on a 100-high page → normalized y=10.
        assert_eq!((r.y0, r.y1), (10.0, 10.0));
        assert_eq!((r.x0, r.x1), (0.0, 50.0));
    }

    #[test]
    fn rect_bbox_from_corners() {
        let r = make_rect([(10.0, 80.0), (60.0, 80.0), (10.0, 95.0), (60.0, 95.0)], 100.0);
        assert_eq!(r.bbox, BBox { x0: 10.0, y0: 5.0, x1: 60.0, y1: 20.0 });
    }
}
