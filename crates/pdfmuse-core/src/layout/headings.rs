//! PDF heading detection (PER-167).
//!
//! Deterministic and geometric: a paragraph whose dominant font size is clearly
//! larger than the document's body text becomes a heading, and heading sizes are
//! ranked into levels (largest = `#`). A conservative second pass catches headings
//! that share the body size but carry an unambiguous *numbering* prefix — `第 X 章`,
//! `一、`, `（一）`, `1.1` — which is on-brand for pdfmuse's CJK-first-class stance.
//!
//! This populates [`Paragraph::heading_level`](crate::ir::Paragraph), which was
//! always `None` on the PDF path — so `heading_path` and Markdown `#` now work for
//! PDF. DOCX headings come from Word styles and are left untouched.

use std::collections::BTreeMap;

use crate::ir::{BBox, Block, Char, Document};

/// A heading's dominant size must exceed the body size by at least this ratio.
const SIZE_RATIO: f32 = 1.15;
/// Headings are short; a large-font paragraph longer than this is treated as body.
const MAX_HEADING_CHARS: usize = 120;
/// A numbering-only heading candidate must be at most this many chars (avoids
/// flagging long numbered list items / sentences).
const MAX_NUMBERED_CHARS: usize = 80;
/// Deepest heading level we assign.
const MAX_LEVEL: u8 = 6;

/// Assign heading levels to every PDF paragraph in `doc` (in place).
pub(crate) fn assign_headings(doc: &mut Document) {
    // Body size = the most common char size across the whole document (ties → the
    // smaller size, which is almost always the body).
    let Some(body_key) = dominant_key(doc.pages.iter().flat_map(|p| p.chars.iter().map(|c| c.size)), false)
    else {
        return;
    };
    let threshold = key_to_pt(body_key) * SIZE_RATIO;

    // Pass 1 — size clustering. Record each large-font short paragraph and the set
    // of distinct heading sizes, so levels are ranked consistently across pages.
    let mut heading_sizes: Vec<i32> = Vec::new();
    let mut sized: Vec<(usize, usize, i32)> = Vec::new(); // (page, block, size_key)
    for (pi, page) in doc.pages.iter().enumerate() {
        for (bi, block) in page.blocks.iter().enumerate() {
            let Block::Paragraph(p) = block else { continue };
            if p.text.trim().is_empty() || p.text.chars().count() > MAX_HEADING_CHARS {
                continue;
            }
            let Some(key) = dominant_key(chars_in(&page.chars, &p.bbox).map(|c| c.size), true) else {
                continue;
            };
            if key_to_pt(key) >= threshold {
                if !heading_sizes.contains(&key) {
                    heading_sizes.push(key);
                }
                sized.push((pi, bi, key));
            }
        }
    }
    // Largest distinct heading size → level 1, next → 2, …
    heading_sizes.sort_unstable_by(|a, b| b.cmp(a));
    for (pi, bi, key) in sized {
        let level = heading_sizes.iter().position(|&s| s == key).map_or(1, |i| i as u8 + 1).min(MAX_LEVEL);
        if let Block::Paragraph(p) = &mut doc.pages[pi].blocks[bi] {
            p.heading_level = Some(level);
        }
    }

    // Pass 2 — numbering prefixes for headings that share the body size.
    for page in &mut doc.pages {
        for block in &mut page.blocks {
            if let Block::Paragraph(p) = block {
                if p.heading_level.is_none() {
                    if let Some(level) = numbering_level(&p.text) {
                        p.heading_level = Some(level);
                    }
                }
            }
        }
    }
}

/// Round a size to a 0.5pt bucket so near-equal sizes cluster together.
fn size_key(size: f32) -> i32 {
    (size * 2.0).round() as i32
}

fn key_to_pt(key: i32) -> f32 {
    key as f32 / 2.0
}

/// Chars whose center lies inside `bbox` — the members of a paragraph.
fn chars_in<'a>(chars: &'a [Char], bbox: &'a BBox) -> impl Iterator<Item = &'a Char> {
    chars.iter().filter(move |c| {
        let cx = (c.bbox.x0 + c.bbox.x1) / 2.0;
        let cy = (c.bbox.y0 + c.bbox.y1) / 2.0;
        cx >= bbox.x0 && cx <= bbox.x1 && cy >= bbox.y0 && cy <= bbox.y1
    })
}

/// The most common size bucket among `sizes`. On a tie, prefer the larger bucket
/// when `prefer_larger` (a paragraph's heading size), else the smaller (body).
fn dominant_key(sizes: impl Iterator<Item = f32>, prefer_larger: bool) -> Option<i32> {
    let mut counts: BTreeMap<i32, u32> = BTreeMap::new();
    for s in sizes {
        *counts.entry(size_key(s)).or_insert(0) += 1;
    }
    let mut best: Option<(i32, u32)> = None;
    for (k, c) in counts {
        // BTreeMap iterates keys ascending, so on a tie `prefer_larger` takes the
        // later (larger) key and otherwise the existing (smaller) one.
        best = match best {
            Some((_, bc)) if c < bc => best,
            Some((_, bc)) if c == bc && !prefer_larger => best,
            _ => Some((k, c)),
        };
    }
    best.map(|(k, _)| k)
}

fn is_cjk_numeral(c: char) -> bool {
    matches!(c, '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百' | '千' | '零' | '〇' | '两')
}

/// A heading level implied by a leading numbering prefix, or `None`. Conservative:
/// only short, standalone lines that don't read like a sentence/list item qualify.
fn numbering_level(text: &str) -> Option<u8> {
    let t = text.trim();
    let chars: Vec<char> = t.chars().collect();
    if chars.is_empty() || chars.len() > MAX_NUMBERED_CHARS {
        return None;
    }
    // A line ending in sentence/list punctuation reads like prose, not a heading.
    if matches!(
        *chars.last().unwrap(),
        '。' | '.' | '!' | '?' | '！' | '？' | ';' | '；' | ',' | '，' | '、' | ':' | '：'
    ) {
        return None;
    }

    // 第 X 章 / 节 …
    if chars[0] == '第' {
        for &c in &chars[1..] {
            match c {
                '章' | '篇' | '部' | '卷' | '编' => return Some(1),
                '节' | '讲' | '回' => return Some(2),
                _ => {}
            }
        }
    }
    // 一、 …  → level 1
    if is_cjk_numeral(chars[0]) {
        let mut i = 0;
        while i < chars.len() && is_cjk_numeral(chars[i]) {
            i += 1;
        }
        if matches!(chars.get(i), Some('、') | Some('.') | Some('．')) {
            return Some(1);
        }
    }
    // （一） …  → level 2
    if matches!(chars[0], '（' | '(') && chars.get(1).is_some_and(|&c| is_cjk_numeral(c)) {
        let mut i = 1;
        while i < chars.len() && is_cjk_numeral(chars[i]) {
            i += 1;
        }
        if matches!(chars.get(i), Some('）') | Some(')')) {
            return Some(2);
        }
    }
    // 1.1 / 1.2.3 …  (a *multi-level* dotted number — bare "1." collides with lists
    // and is left to the size pass). Level = number of dotted parts.
    if chars[0].is_ascii_digit() {
        let token: String = t.chars().take_while(|c| !c.is_whitespace()).collect();
        let token = token.trim_end_matches('.');
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
            return Some((parts.len() as u8).min(MAX_LEVEL));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{FontRef, Page, Paragraph};

    fn ch(text: &str, x0: f32, x1: f32, y0: f32, y1: f32, size: f32) -> Char {
        Char { text: text.into(), bbox: BBox { x0, y0, x1, y1 }, font: FontRef::default(), size, color: None }
    }

    fn para(text: &str, bbox: BBox) -> Block {
        Block::Paragraph(Paragraph { bbox, text: text.into(), heading_level: None })
    }

    fn level_of(b: &Block) -> Option<u8> {
        match b {
            Block::Paragraph(p) => p.heading_level,
            _ => None,
        }
    }

    /// A doc with a 20pt title over 10pt body → title is H1, body stays None.
    #[test]
    fn size_clustering_flags_large_font_as_heading() {
        let title_bb = BBox { x0: 0.0, y0: 0.0, x1: 100.0, y1: 20.0 };
        let body_bb = BBox { x0: 0.0, y0: 30.0, x1: 100.0, y1: 40.0 };
        let mut chars = vec![ch("Title", 0.0, 100.0, 0.0, 20.0, 20.0)];
        // plenty of body chars so 10pt is the mode
        for i in 0..20 {
            chars.push(ch("x", i as f32, i as f32 + 1.0, 30.0, 40.0, 10.0));
        }
        let mut doc = Document::default();
        doc.pages.push(Page { chars, blocks: vec![para("Title", title_bb), para("body text here", body_bb)], ..Page::default() });
        assign_headings(&mut doc);
        assert_eq!(level_of(&doc.pages[0].blocks[0]), Some(1));
        assert_eq!(level_of(&doc.pages[0].blocks[1]), None);
    }

    /// Two heading sizes → ranked into H1 and H2.
    #[test]
    fn distinct_sizes_rank_into_levels() {
        let mut chars = vec![
            ch("Big", 0.0, 30.0, 0.0, 18.0, 18.0),
            ch("Mid", 0.0, 30.0, 30.0, 44.0, 14.0),
        ];
        for i in 0..20 {
            chars.push(ch("x", i as f32, i as f32 + 1.0, 60.0, 70.0, 10.0));
        }
        let mut doc = Document::default();
        doc.pages.push(Page {
            chars,
            blocks: vec![
                para("Big", BBox { x0: 0.0, y0: 0.0, x1: 30.0, y1: 18.0 }),
                para("Mid", BBox { x0: 0.0, y0: 30.0, x1: 30.0, y1: 44.0 }),
                para("body", BBox { x0: 0.0, y0: 60.0, x1: 20.0, y1: 70.0 }),
            ],
            ..Page::default()
        });
        assign_headings(&mut doc);
        assert_eq!(level_of(&doc.pages[0].blocks[0]), Some(1));
        assert_eq!(level_of(&doc.pages[0].blocks[1]), Some(2));
        assert_eq!(level_of(&doc.pages[0].blocks[2]), None);
    }

    #[test]
    fn numbering_patterns() {
        assert_eq!(numbering_level("第一章 绪论"), Some(1));
        assert_eq!(numbering_level("第二节 方法"), Some(2));
        assert_eq!(numbering_level("一、研究背景"), Some(1));
        assert_eq!(numbering_level("（三）局限"), Some(2));
        assert_eq!(numbering_level("1.1 Background"), Some(2));
        assert_eq!(numbering_level("2.3.1 Details"), Some(3));
    }

    #[test]
    fn numbering_rejects_lists_and_prose() {
        // bare "1." collides with lists — left to the size pass
        assert_eq!(numbering_level("1. First bullet point"), None);
        // sentence-ending punctuation ⇒ prose, not a heading
        assert_eq!(numbering_level("一、这是一句完整的话。"), None);
        assert_eq!(numbering_level("这是正文段落,不是标题"), None);
    }
}
