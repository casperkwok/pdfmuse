//! Line → paragraph grouping.
//!
//! Consecutive lines (already ordered top-to-bottom) are grouped into paragraphs;
//! a vertical gap noticeably larger than the line height starts a new paragraph.
//! Fully geometric and deterministic.

use crate::ir::{BBox, Block, Paragraph, TextLine};

/// A vertical gap above this fraction of the line height starts a new paragraph.
const PARA_GAP: f32 = 0.6;

/// Group ordered `lines` into paragraph blocks.
pub(super) fn group_paragraphs(lines: &[TextLine]) -> Vec<Block> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut blocks = Vec::new();
    let mut start = 0;
    for i in 1..lines.len() {
        let prev = &lines[i - 1];
        let height = (prev.bbox.y1 - prev.bbox.y0).max(1.0);
        let gap = lines[i].bbox.y0 - prev.bbox.y1;
        if gap > PARA_GAP * height {
            blocks.push(make_paragraph(&lines[start..i]));
            start = i;
        }
    }
    blocks.push(make_paragraph(&lines[start..]));
    blocks
}

fn make_paragraph(lines: &[TextLine]) -> Block {
    let text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
    let mut bbox: Option<BBox> = None;
    for l in lines {
        bbox = Some(match bbox {
            None => l.bbox,
            Some(b) => BBox {
                x0: b.x0.min(l.bbox.x0),
                y0: b.y0.min(l.bbox.y0),
                x1: b.x1.max(l.bbox.x1),
                y1: b.y1.max(l.bbox.y1),
            },
        });
    }
    Block::Paragraph(Paragraph { bbox: bbox.unwrap_or_default(), text, heading_level: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, y0: f32, y1: f32) -> TextLine {
        TextLine { bbox: BBox { x0: 0.0, y0, x1: 100.0, y1 }, text: text.into(), chars: vec![] }
    }

    #[test]
    fn tight_lines_form_one_paragraph() {
        // Two 10pt-high lines, 2pt apart → same paragraph.
        let lines = vec![line("first", 0.0, 10.0), line("second", 12.0, 22.0)];
        let blocks = group_paragraphs(&lines);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::Paragraph(p) => assert_eq!(p.text, "first second"),
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn wide_gap_splits_paragraphs() {
        // Second line 20pt below the first (gap 20 > 0.6×10) → two paragraphs.
        let lines = vec![line("para one", 0.0, 10.0), line("para two", 30.0, 40.0)];
        let blocks = group_paragraphs(&lines);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn empty_lines_yield_no_blocks() {
        assert!(group_paragraphs(&[]).is_empty());
    }
}
