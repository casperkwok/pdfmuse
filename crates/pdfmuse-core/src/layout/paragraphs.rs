//! Line → paragraph grouping.
//!
//! Consecutive lines (already ordered top-to-bottom) are grouped into paragraphs;
//! a vertical gap noticeably larger than the line height starts a new paragraph.
//! Fully geometric and deterministic.

use crate::ir::{BBox, Block, Char, Paragraph, TextLine};

/// A vertical gap above this fraction of the line height starts a new paragraph.
const PARA_GAP: f32 = 0.6;
/// Adjacent lines whose dominant font sizes differ by more than this ratio start a
/// new paragraph — so a heading is never glued to the body text beneath it (which
/// would hide the heading's larger size behind the body's). See PER-167.
const SIZE_SPLIT_RATIO: f32 = 1.15;

/// Group ordered `lines` into paragraph blocks. `chars` backs each line's font
/// size (via its char indices) so a size change can break a paragraph.
pub(super) fn group_paragraphs(lines: &[TextLine], chars: &[Char]) -> Vec<Block> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut blocks = Vec::new();
    let mut start = 0;
    for i in 1..lines.len() {
        let prev = &lines[i - 1];
        let height = (prev.bbox.y1 - prev.bbox.y0).max(1.0);
        let gap = lines[i].bbox.y0 - prev.bbox.y1;
        if gap > PARA_GAP * height || size_break(prev, &lines[i], chars) {
            blocks.push(make_paragraph(&lines[start..i]));
            start = i;
        }
    }
    blocks.push(make_paragraph(&lines[start..]));
    blocks
}

/// True when two lines' dominant font sizes differ enough to be different roles.
fn size_break(a: &TextLine, b: &TextLine, chars: &[Char]) -> bool {
    match (line_size(a, chars), line_size(b, chars)) {
        (Some(sa), Some(sb)) => {
            let (lo, hi) = if sa < sb { (sa, sb) } else { (sb, sa) };
            lo > 0.0 && hi / lo > SIZE_SPLIT_RATIO
        }
        _ => false,
    }
}

/// A line's dominant char size (mode over a 0.5pt bucket), or `None` if the line
/// carries no char indices (e.g. synthetic test lines) — then no size split.
fn line_size(line: &TextLine, chars: &[Char]) -> Option<f32> {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<i32, u32> = BTreeMap::new();
    for &i in &line.chars {
        if let Some(c) = chars.get(i as usize) {
            *counts.entry((c.size * 2.0).round() as i32).or_insert(0) += 1;
        }
    }
    counts.into_iter().max_by_key(|&(_, c)| c).map(|(k, _)| k as f32 / 2.0)
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
        let blocks = group_paragraphs(&lines, &[]);
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
        let blocks = group_paragraphs(&lines, &[]);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn empty_lines_yield_no_blocks() {
        assert!(group_paragraphs(&[], &[]).is_empty());
    }
}
