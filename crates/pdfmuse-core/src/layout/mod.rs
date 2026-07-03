//! Geometric layout analysis.
//!
//! Turns a page's positioned [`Char`](crate::ir::Char)s into higher-level
//! structure — lines, then paragraphs, in reading order — using deterministic
//! geometric rules only (no models). Each stage lands in its own issue:
//! lines (PER-40), paragraphs (PER-41), columns/reading-order (PER-42),
//! tables (PER-43).

mod columns;
mod headings;
mod lines;
mod paragraphs;
mod tables;

pub(crate) use headings::assign_headings;

use crate::ir::{Block, Page, TextLine};

/// Populate a page's layered structure from its `chars` (+ rules/rects).
///
/// Columns are detected from char positions **before** line clustering, so each
/// column is clustered independently — a two-column page no longer merges its
/// columns onto shared baselines. Blocks come out in reading order: each column
/// top-to-bottom, columns left-to-right.
pub(crate) fn layout_page(page: &mut Page) {
    // Ruled tables first — their chars are consumed by the table, not the text flow.
    let ruled = tables::detect_ruled(&page.chars, &page.rects, &page.rules);
    let ruled_mask: Vec<bool> = page
        .chars
        .iter()
        .map(|c| {
            let cx = (c.bbox.x0 + c.bbox.x1) / 2.0;
            let cy = (c.bbox.y0 + c.bbox.y1) / 2.0;
            ruled.iter().any(|t| cx >= t.bbox.x0 && cx <= t.bbox.x1 && cy >= t.bbox.y0 && cy <= t.bbox.y1)
        })
        .collect();

    let splits = columns::detect_columns(&page.chars, &ruled_mask);
    let ncols = splits.len() + 1;

    let mut all_lines: Vec<TextLine> = Vec::new();
    let mut col_blocks: Vec<Block> = Vec::new();
    for col in 0..ncols {
        // This column's chars = not in a ruled table and whose center is in this band.
        let band_mask: Vec<bool> = page
            .chars
            .iter()
            .enumerate()
            .map(|(i, c)| ruled_mask[i] || columns::column_index((c.bbox.x0 + c.bbox.x1) / 2.0, &splits) != col)
            .collect();

        let lines = lines::cluster_lines(&page.chars, &band_mask);
        // Whitespace-aligned tables among this column's lines.
        let (ws_tables, used) = tables::detect_whitespace(&page.chars, &lines);
        let para_lines: Vec<TextLine> =
            lines.iter().enumerate().filter(|(i, _)| !used.contains(i)).map(|(_, l)| l.clone()).collect();

        let mut blocks = paragraphs::group_paragraphs(&para_lines, &page.chars);
        blocks.extend(ws_tables.into_iter().map(Block::Table));
        blocks.sort_by(|a, b| columns::block_top(a).total_cmp(&columns::block_top(b)));
        col_blocks.extend(blocks);
        all_lines.extend(lines);
    }
    page.lines = all_lines;

    let mut blocks = col_blocks;
    blocks.extend(ruled.into_iter().map(Block::Table));
    // Single column: order everything (including any full-width ruled table) by y.
    if ncols == 1 {
        blocks = columns::reading_order(blocks, page.width);
    }
    page.blocks = blocks;
}
