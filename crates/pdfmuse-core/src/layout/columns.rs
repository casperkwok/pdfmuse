//! Column detection and reading order.
//!
//! Reorders blocks into reading order. The default is top-to-bottom. When the
//! blocks split cleanly into left/right groups with none spanning the middle,
//! they are treated as two columns and ordered column-by-column (left column
//! top-to-bottom, then right). Heuristic but deterministic. Multi-column
//! generalization beyond two is future work.

use crate::ir::{BBox, Block};

/// Reorder `blocks` into reading order for a page of the given width.
pub(super) fn reading_order(mut blocks: Vec<Block>, page_width: f32) -> Vec<Block> {
    if blocks.len() <= 1 {
        return blocks;
    }

    if page_width > 0.0 {
        let mid = page_width / 2.0;
        let margin = 0.05 * page_width;
        // A block that clearly straddles the midline ⇒ full width ⇒ single column.
        let spans_mid = blocks.iter().any(|b| {
            let bb = bbox(b);
            bb.x0 < mid - margin && bb.x1 > mid + margin
        });
        if !spans_mid {
            let (mut left, mut right): (Vec<Block>, Vec<Block>) =
                blocks.into_iter().partition(|b| center_x(bbox(b)) < mid);
            if !left.is_empty() && !right.is_empty() {
                left.sort_by(|a, b| top(a).total_cmp(&top(b)));
                right.sort_by(|a, b| top(a).total_cmp(&top(b)));
                left.extend(right);
                return left;
            }
            // All on one side — fall through to single-column ordering.
            blocks = left.into_iter().chain(right).collect();
        }
    }

    blocks.sort_by(|a, b| top(a).total_cmp(&top(b)));
    blocks
}

fn bbox(b: &Block) -> BBox {
    match b {
        Block::Paragraph(p) => p.bbox,
        Block::Table(t) => t.bbox,
        Block::Image(i) => i.bbox,
    }
}

fn center_x(b: BBox) -> f32 {
    (b.x0 + b.x1) / 2.0
}

fn top(b: &Block) -> f32 {
    bbox(b).y0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Paragraph;

    fn para(label: &str, x0: f32, x1: f32, y0: f32) -> Block {
        Block::Paragraph(Paragraph {
            bbox: BBox { x0, y0, x1, y1: y0 + 10.0 },
            text: label.into(),
            heading_level: None,
        })
    }

    fn text(b: &Block) -> &str {
        match b {
            Block::Paragraph(p) => &p.text,
            _ => "",
        }
    }

    #[test]
    fn two_columns_ordered_left_then_right() {
        // Left column x∈[0,200], right column x∈[300,500] on a 500-wide page.
        let blocks = vec![
            para("R-top", 300.0, 500.0, 0.0),
            para("L-bottom", 0.0, 200.0, 50.0),
            para("L-top", 0.0, 200.0, 0.0),
            para("R-bottom", 300.0, 500.0, 50.0),
        ];
        let ordered = reading_order(blocks, 500.0);
        let got: Vec<&str> = ordered.iter().map(text).collect();
        assert_eq!(got, vec!["L-top", "L-bottom", "R-top", "R-bottom"]);
    }

    #[test]
    fn full_width_block_stays_single_column() {
        // A full-width heading must not trigger a 2-column split.
        let blocks = vec![
            para("body", 0.0, 200.0, 50.0),
            para("heading", 0.0, 500.0, 0.0), // spans the midline
        ];
        let ordered = reading_order(blocks, 500.0);
        let got: Vec<&str> = ordered.iter().map(text).collect();
        assert_eq!(got, vec!["heading", "body"]); // pure top-to-bottom
    }

    #[test]
    fn single_block_unchanged() {
        let blocks = vec![para("only", 0.0, 200.0, 0.0)];
        assert_eq!(reading_order(blocks, 500.0).len(), 1);
    }
}
