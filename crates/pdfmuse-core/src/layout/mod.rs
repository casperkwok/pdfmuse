//! Geometric layout analysis.
//!
//! Turns a page's positioned [`Char`](crate::ir::Char)s into higher-level
//! structure — lines, then paragraphs, in reading order — using deterministic
//! geometric rules only (no models). Each stage lands in its own issue:
//! lines (PER-40), paragraphs (PER-41), columns/reading-order (PER-42),
//! tables (PER-43).

mod lines;

use crate::ir::Page;

/// Populate a page's layered structure from its `chars`.
pub(crate) fn layout_page(page: &mut Page) {
    page.lines = lines::cluster_lines(&page.chars);
}
