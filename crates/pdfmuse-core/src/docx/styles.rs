//! Resolve DOCX paragraph style ids to heading levels.
//!
//! A paragraph references a style by id (`w:pPr/w:pStyle @w:val`). A style is a
//! heading when its display name is `heading 1`..`heading 9` (Word's convention,
//! case-insensitive) or its id looks like `Heading{N}`. We build a
//! `styleId → level` map from `word/styles.xml`; [`heading_level`] falls back to
//! interpreting a `Heading{N}` id directly, so headings still resolve when that
//! part is missing.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::Result;

use super::{attr_by_local, next_event};

/// Parse `word/styles.xml` into a `styleId → heading level (1..=9)` map.
pub(super) fn parse_styles(xml: &[u8]) -> Result<HashMap<String, u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut map = HashMap::new();
    let mut current_id: Option<String> = None;
    loop {
        match next_event(&mut reader, &mut buf)? {
            Event::Start(e) if e.local_name().as_ref() == b"style" => {
                current_id = attr_by_local(&e, b"styleId");
            }
            // `w:name` is typically self-closing (`Empty`); accept both forms.
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"name" => {
                if let Some(id) = &current_id {
                    let name = attr_by_local(&e, b"val").unwrap_or_default();
                    if let Some(level) = level_from_name(&name).or_else(|| level_from_id(id)) {
                        map.insert(id.clone(), level);
                    }
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"style" => current_id = None,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(map)
}

/// Resolve a paragraph's `pStyle` id to a heading level.
///
/// Prefers the map built from `styles.xml`, then falls back to reading a
/// `Heading{N}` style id directly.
pub(super) fn heading_level(styles: &HashMap<String, u8>, style_id: &str) -> Option<u8> {
    if let Some(&level) = styles.get(style_id) {
        return Some(level);
    }
    level_from_id(style_id)
}

/// `"heading 3"` → `3` (Word's display name form, case-insensitive).
fn level_from_name(name: &str) -> Option<u8> {
    let lower = name.trim().to_ascii_lowercase();
    let digits = lower.strip_prefix("heading")?;
    valid_level(digits.trim())
}

/// `"Heading3"` → `3` (the style-id form, case-insensitive).
fn level_from_id(id: &str) -> Option<u8> {
    let lower = id.to_ascii_lowercase();
    let digits = lower.strip_prefix("heading")?;
    valid_level(digits)
}

/// Accept only levels 1..=9 (Word's heading range).
fn valid_level(digits: &str) -> Option<u8> {
    match digits.parse::<u8>() {
        Ok(n) if (1..=9).contains(&n) => Some(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_name_and_id_forms() {
        let styles = parse_styles(
            br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:style w:styleId="H2"><w:name w:val="heading 2"/></w:style>
                <w:style w:styleId="Body"><w:name w:val="Body Text"/></w:style>
            </w:styles>"#,
        )
        .unwrap();

        // Named "heading 2" → level 2 under its custom id.
        assert_eq!(heading_level(&styles, "H2"), Some(2));
        // Non-heading style → None.
        assert_eq!(heading_level(&styles, "Body"), None);
        // Unknown id that looks like `Heading{N}` resolves via the fallback.
        assert_eq!(heading_level(&styles, "Heading4"), Some(4));
        assert_eq!(heading_level(&styles, "Normal"), None);
        // Out-of-range levels are rejected.
        assert_eq!(heading_level(&HashMap::new(), "Heading12"), None);
    }
}
