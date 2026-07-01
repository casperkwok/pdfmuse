//! Geometric layout analysis.
//!
//! Turns a page's positioned [`Char`](crate::ir::Char)s into higher-level
//! structure — lines, then paragraphs, in reading order — using deterministic
//! geometric rules only (no models). Each stage lands in its own issue:
//! lines (PER-40), paragraphs (PER-41), columns/reading-order (PER-42),
//! tables (PER-43).

mod columns;
mod lines;
mod paragraphs;
mod tables;

use crate::ir::{Block, Page};

/// Populate a page's layered structure from its `chars` (+ rules/rects).
pub(crate) fn layout_page(page: &mut Page) {
    // Ruled tables first — their chars are consumed by the table, not the text flow.
    let tables = tables::detect_ruled(&page.chars, &page.rects, &page.rules);
    let skip: Vec<bool> = page
        .chars
        .iter()
        .map(|c| {
            let cx = (c.bbox.x0 + c.bbox.x1) / 2.0;
            let cy = (c.bbox.y0 + c.bbox.y1) / 2.0;
            tables.iter().any(|t| cx >= t.bbox.x0 && cx <= t.bbox.x1 && cy >= t.bbox.y0 && cy <= t.bbox.y1)
        })
        .collect();

    page.lines = lines::cluster_lines(&page.chars, &skip);
    let mut blocks = paragraphs::group_paragraphs(&page.lines);
    blocks.extend(tables.into_iter().map(Block::Table));
    page.blocks = columns::reading_order(blocks, page.width);
}
