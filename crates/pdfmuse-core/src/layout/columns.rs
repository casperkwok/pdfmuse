//! Column detection and reading order.
//!
//! Reorders blocks into reading order. The default is top-to-bottom. When the
//! blocks split cleanly into left/right groups with none spanning the middle,
//! they are treated as two columns and ordered column-by-column (left column
//! top-to-bottom, then right). Heuristic but deterministic. Multi-column
//! generalization beyond two is future work.

use crate::ir::{BBox, Block, Char};

/// A gutter must be at least this wide (× font size, floored to an absolute) to
/// count as a column separator rather than a word/indent gap.
const GUTTER_SIZE: f32 = 2.0;
const GUTTER_MIN_PT: f32 = 12.0;

/// Detect vertical column gutters from char positions, returning the x-positions
/// that split the page into columns (empty ⇒ single column). A gutter is a wide
/// **interior** vertical strip that no character crosses — the signature of
/// multi-column text. Detecting columns *before* line clustering is what stops a
/// two-column page from interleaving its columns onto shared baselines.
pub(super) fn detect_columns(chars: &[Char], skip: &[bool]) -> Vec<f32> {
    let active: Vec<&Char> = chars.iter().zip(skip).filter(|(_, &s)| !s).map(|(c, _)| c).collect();
    if active.len() < 12 {
        return Vec::new();
    }
    let minx = active.iter().map(|c| c.bbox.x0).fold(f32::MAX, f32::min);
    let maxx = active.iter().map(|c| c.bbox.x1).fold(f32::MIN, f32::max);
    let miny = active.iter().map(|c| c.bbox.y0).fold(f32::MAX, f32::min);
    let maxy = active.iter().map(|c| c.bbox.y1).fold(f32::MIN, f32::max);
    if maxx <= minx || maxy <= miny {
        return Vec::new();
    }
    let mut sizes: Vec<f32> = active.iter().map(|c| c.size).collect();
    sizes.sort_by(f32::total_cmp);
    let size = sizes[sizes.len() / 2].max(1.0);
    let gutter_min = (GUTTER_SIZE * size).max(GUTTER_MIN_PT);

    // Row-band coverage: for each 1pt x-bucket, which text rows have ink there.
    // A gutter is empty in *most* rows — so a full-width header (one row that
    // spans the gutter) doesn't hide it.
    let band_h = (1.2 * size).max(1.0);
    let nrow = ((maxy - miny) / band_h).ceil() as usize + 1;
    let nx = (maxx - minx).ceil() as usize + 1;
    let mut covered = vec![vec![false; nrow]; nx];
    let mut used_rows = vec![false; nrow];
    for c in &active {
        let cy = (c.bbox.y0 + c.bbox.y1) / 2.0;
        let row = (((cy - miny) / band_h) as usize).min(nrow - 1);
        used_rows[row] = true;
        let a = ((c.bbox.x0 - minx).floor().max(0.0) as usize).min(nx - 1);
        let b = ((c.bbox.x1 - minx).ceil() as usize).min(nx - 1);
        for x in covered.iter_mut().take(b + 1).skip(a) {
            x[row] = true;
        }
    }
    let total_rows = used_rows.iter().filter(|&&u| u).count().max(1);
    let thresh = 0.15 * total_rows as f32;
    let row_count: Vec<usize> = covered.iter().map(|rows| rows.iter().filter(|&&b| b).count()).collect();

    // The text region is between the first and last well-covered x-bucket.
    let first = row_count.iter().position(|&c| c as f32 > thresh);
    let last = row_count.iter().rposition(|&c| c as f32 > thresh);
    let (Some(first), Some(last)) = (first, last) else {
        return Vec::new();
    };

    // Interior runs that stay below the threshold are gutters; split at the center.
    let mut splits = Vec::new();
    let mut k = first;
    while k <= last {
        if row_count[k] as f32 > thresh {
            k += 1;
            continue;
        }
        let start = k;
        while k <= last && (row_count[k] as f32) <= thresh {
            k += 1;
        }
        if (k - start) as f32 >= gutter_min {
            splits.push(minx + (start + k) as f32 / 2.0);
        }
    }
    splits
}

/// The column index (0-based, left→right) of an x-center given the split lines.
pub(super) fn column_index(cx: f32, splits: &[f32]) -> usize {
    splits.iter().filter(|&&s| cx > s).count()
}

/// Top edge (`y0`) of a block, for top-to-bottom ordering within a column.
pub(super) fn block_top(b: &Block) -> f32 {
    bbox(b).y0
}

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
            heading_level: None, role: None,
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

    fn glyph(x: f32, baseline: f32) -> Char {
        Char {
            text: "x".into(),
            bbox: BBox { x0: x, y0: baseline - 10.0, x1: x + 6.0, y1: baseline },
            font: crate::ir::FontRef { name: "F".into() },
            size: 10.0,
            color: None,
        }
    }

    #[test]
    fn detects_two_columns_despite_full_width_header() {
        // A full-width header row (crosses the gutter) plus a 2-column body:
        // left x∈[0,80], gutter [80,120], right x∈[120,200], over many rows.
        let mut chars = Vec::new();
        for x in (0..200).step_by(6) {
            chars.push(glyph(x as f32, 0.0)); // header spans the whole width
        }
        for row in 1..15 {
            let y = row as f32 * 12.0;
            for x in (0..80).step_by(6) {
                chars.push(glyph(x as f32, y));
            }
            for x in (120..200).step_by(6) {
                chars.push(glyph(x as f32, y));
            }
        }
        let skip = vec![false; chars.len()];
        let splits = detect_columns(&chars, &skip);
        assert_eq!(splits.len(), 1, "expected one gutter, got {splits:?}");
        assert!(splits[0] > 80.0 && splits[0] < 120.0, "gutter at {}", splits[0]);
    }

    #[test]
    fn single_column_has_no_splits() {
        let mut chars = Vec::new();
        for row in 0..15 {
            for x in (0..200).step_by(6) {
                chars.push(glyph(x as f32, row as f32 * 12.0));
            }
        }
        let skip = vec![false; chars.len()];
        assert!(detect_columns(&chars, &skip).is_empty());
    }
}
