//! Running header/footer detection (PER-168).
//!
//! Deterministic and geometric: a short paragraph that repeats — modulo a page
//! number — in the same top/bottom edge band across a majority of a multi-page
//! document is a running header/footer. Such paragraphs are *marked*
//! ([`BlockRole::HeaderFooter`]) but never dropped here — honoring the "never
//! silently drop content" rule. [`remove_boilerplate`](crate::remove_boilerplate)
//! is the caller's opt-in switch to strip them.

use std::collections::HashMap;

use crate::ir::{Block, BlockRole, Document};

/// Top and bottom band, as a fraction of page height, where running heads/feet live.
const EDGE_FRAC: f32 = 0.12;
/// The fewest pages a multi-page doc must have before we look for running elements.
const MIN_PAGES: usize = 3;

/// Mark running headers/footers across `doc` (in place). No-op for short documents.
pub(crate) fn mark(doc: &mut Document) {
    let npages = doc.pages.len();
    if npages < MIN_PAGES {
        return;
    }
    // A running element must appear on at least a majority of pages.
    let min_pages = npages.div_ceil(2);

    // (band, normalized-text) → the (page, block) occurrences carrying it.
    let mut seen: HashMap<(u8, String), Vec<(usize, usize)>> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        if page.height <= 0.0 {
            continue;
        }
        let top = page.height * EDGE_FRAC;
        let bottom = page.height * (1.0 - EDGE_FRAC);
        for (bi, block) in page.blocks.iter().enumerate() {
            let Block::Paragraph(p) = block else { continue };
            let cy = (p.bbox.y0 + p.bbox.y1) / 2.0;
            let band = if cy < top {
                0u8
            } else if cy > bottom {
                1u8
            } else {
                continue; // body region — never boilerplate
            };
            let norm = normalize(&p.text);
            if !norm.is_empty() {
                seen.entry((band, norm)).or_default().push((pi, bi));
            }
        }
    }

    for occ in seen.into_values() {
        let mut pages: Vec<usize> = occ.iter().map(|&(p, _)| p).collect();
        pages.sort_unstable();
        pages.dedup();
        if pages.len() >= min_pages {
            for (pi, bi) in occ {
                if let Block::Paragraph(p) = &mut doc.pages[pi].blocks[bi] {
                    p.role = Some(BlockRole::HeaderFooter);
                }
            }
        }
    }
}

/// Normalize a line for cross-page matching: collapse each run of ASCII digits to
/// `#` (so "Page 3" and "Page 4" match) and squeeze whitespace.
fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut prev_digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !prev_digit {
                out.push('#');
            }
            prev_digit = true;
        } else {
            prev_digit = false;
            if c.is_whitespace() {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            } else {
                out.push(c);
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BBox, Page, Paragraph};

    fn page(index: u32, header: &str, footer: &str, body: &str) -> Page {
        let para = |text: &str, y0: f32, y1: f32| {
            Block::Paragraph(Paragraph {
                bbox: BBox { x0: 0.0, y0, x1: 100.0, y1 },
                text: text.into(),
                heading_level: None,
                role: None,
            })
        };
        Page {
            index,
            height: 800.0,
            blocks: vec![para(header, 10.0, 24.0), para(body, 400.0, 414.0), para(footer, 770.0, 784.0)],
            ..Default::default()
        }
    }

    fn role_of(b: &Block) -> Option<BlockRole> {
        match b {
            Block::Paragraph(p) => p.role,
            _ => None,
        }
    }

    #[test]
    fn marks_repeated_header_and_footer_but_not_body() {
        let mut doc = Document {
            pages: vec![
                page(0, "Annual Report", "Page 1", "Introduction section here"),
                page(1, "Annual Report", "Page 2", "Methods section here"),
                page(2, "Annual Report", "Page 3", "Results section here"),
            ],
            ..Default::default()
        };
        mark(&mut doc);
        for (pi, page) in doc.pages.iter().enumerate() {
            assert_eq!(role_of(&page.blocks[0]), Some(BlockRole::HeaderFooter), "header p{pi}");
            assert_eq!(role_of(&page.blocks[2]), Some(BlockRole::HeaderFooter), "footer p{pi} (page #)");
            assert_eq!(role_of(&page.blocks[1]), None, "body p{pi} must be kept");
        }
    }

    #[test]
    fn single_and_two_page_docs_are_untouched() {
        let mut doc = Document {
            pages: vec![page(0, "Hdr", "Ftr", "body"), page(1, "Hdr", "Ftr", "body")],
            ..Default::default()
        };
        mark(&mut doc);
        assert!(doc.pages.iter().all(|pg| pg.blocks.iter().all(|b| role_of(b).is_none())));
    }
}
