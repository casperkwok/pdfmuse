//! Font handling for **simple** (Type1 / TrueType) fonts.
//!
//! Maps each byte code to a Unicode string (base encoding + `Differences` + the
//! Adobe Glyph List) and to an advance width (font `Widths` array, else Core-14
//! metrics, else a fallback). CID / Type0 (CJK) fonts are deferred to M2
//! (PER-46/47); [`Font::is_cid`] flags them so the interpreter (PER-36) can emit
//! a `MissingCMap` warning instead of guessing.
//!
use lopdf::{Dictionary, Document, Object};

use super::tables::{agl::AGL, encodings, metrics};

/// Fallback advance width (1/1000 em) when nothing better is known.
const DEFAULT_WIDTH: f32 = 500.0;

pub(crate) struct Font {
    glyphs: Vec<Glyph>, // indexed by byte code 0..=255
    /// BaseFont name, used to label chars in the IR.
    pub(crate) base: String,
    pub(crate) is_cid: bool,
}

#[derive(Clone, Default)]
struct Glyph {
    text: Option<String>,
    width: f32, // 1/1000 em
}

impl Font {
    /// Build a font model from its dictionary.
    pub(crate) fn from_dict(doc: &Document, dict: &Dictionary) -> Font {
        let base = dict
            .get(b"BaseFont")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_default();

        let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()).unwrap_or(b"");
        if subtype == b"Type0" {
            // CID font — real CMap handling is M2. Flag it and stop.
            return Font { glyphs: vec![Glyph::default(); 256], base, is_cid: true };
        }

        let names = encoding_names(doc, dict, &base);
        let widths = resolve_widths(doc, dict, &base, &names);

        let glyphs = (0..256)
            .map(|c| Glyph {
                text: name_to_unicode(&names[c]),
                width: widths[c],
            })
            .collect();
        Font { glyphs, base, is_cid: false }
    }

    /// `(unicode text, advance width in 1/1000 em)` for a byte code.
    pub(crate) fn decode(&self, code: u8) -> (Option<&str>, f32) {
        let g = &self.glyphs[code as usize];
        (g.text.as_deref(), g.width)
    }
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|(_, x)| x).unwrap_or(o)
}

fn number(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

/// The 256-entry base encoding for a font, with `Differences` applied.
fn encoding_names(doc: &Document, dict: &Dictionary, base: &str) -> Vec<String> {
    let mut names: Vec<String> = base_encoding(doc, dict, base).iter().map(|s| s.to_string()).collect();

    if let Ok(enc) = dict.get(b"Encoding") {
        if let Object::Dictionary(enc_dict) = deref(doc, enc) {
            if let Ok(Object::Array(diffs)) = enc_dict.get(b"Differences").map(|o| deref(doc, o)) {
                apply_differences(diffs, &mut names);
            }
        }
    }
    names
}

fn base_encoding(doc: &Document, dict: &Dictionary, base: &str) -> &'static [&'static str; 256] {
    let named = match dict.get(b"Encoding").ok() {
        Some(Object::Name(n)) => Some(n.clone()),
        Some(o) => deref(doc, o)
            .as_dict()
            .ok()
            .and_then(|d| d.get(b"BaseEncoding").ok())
            .and_then(|b| b.as_name().ok())
            .map(|n| n.to_vec()),
        None => None,
    };
    match named.as_deref() {
        Some(b"WinAnsiEncoding") => &encodings::WIN_ANSI,
        Some(b"MacRomanEncoding") => &encodings::MAC_ROMAN,
        Some(b"StandardEncoding") => &encodings::STANDARD,
        _ if base.contains("ZapfDingbats") => &encodings::ZAPF_DINGBATS,
        // Nominal default for a non-symbolic Type1 font.
        _ => &encodings::STANDARD,
    }
}

fn apply_differences(diffs: &[Object], names: &mut [String]) {
    let mut code = 0usize;
    for item in diffs {
        match item {
            Object::Integer(n) => code = (*n).max(0) as usize,
            Object::Name(name) => {
                if code < 256 {
                    names[code] = String::from_utf8_lossy(name).into_owned();
                    code += 1;
                }
            }
            _ => {}
        }
    }
}

/// Resolve a glyph name to a Unicode string: AGL, then the `uniXXXX` / `uXXXXXX`
/// algorithmic conventions.
fn name_to_unicode(name: &str) -> Option<String> {
    if name.is_empty() || name == ".notdef" {
        return None;
    }
    // Strip a glyph-name suffix like "A.sc" → "A".
    let core = name.split('.').next().unwrap_or(name);
    if core.is_empty() {
        return None;
    }
    if let Ok(i) = AGL.binary_search_by_key(&core, |(n, _)| *n) {
        return Some(AGL[i].1.to_string());
    }
    if let Some(hex) = core.strip_prefix("uni") {
        if hex.len() % 4 == 0 && !hex.is_empty() {
            let mut s = String::new();
            for chunk in hex.as_bytes().chunks(4) {
                let cp = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
                s.push(char::from_u32(cp)?);
            }
            return Some(s);
        }
    }
    if let Some(hex) = core.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) {
            let cp = u32::from_str_radix(hex, 16).ok()?;
            return char::from_u32(cp).map(|c| c.to_string());
        }
    }
    None
}

/// Advance widths (1/1000 em) for codes 0..=255.
fn resolve_widths(doc: &Document, dict: &Dictionary, base: &str, names: &[String]) -> Vec<f32> {
    let first_char = dict.get(b"FirstChar").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
    let missing = dict
        .get(b"MissingWidth")
        .ok()
        .map(|o| number(deref(doc, o)))
        .unwrap_or(0.0);

    // 1) Explicit Widths array (most embedded fonts).
    if let Ok(widths_obj) = dict.get(b"Widths") {
        if let Object::Array(ws) = deref(doc, widths_obj) {
            return (0..256)
                .map(|c| {
                    let idx = c as i64 - first_char;
                    if idx >= 0 && (idx as usize) < ws.len() {
                        number(deref(doc, &ws[idx as usize]))
                    } else if missing > 0.0 {
                        missing
                    } else {
                        DEFAULT_WIDTH
                    }
                })
                .collect();
        }
    }

    // 2) No Widths → Core-14 metrics by canonical base font name.
    let canon = canonical_base(base);
    if canon.starts_with("Courier") {
        return vec![metrics::COURIER_WIDTH as f32; 256];
    }
    if let Some(table) = metrics::std14_metrics(&canon) {
        return names
            .iter()
            .map(|name| lookup_metric(table, name).unwrap_or(if missing > 0.0 { missing } else { DEFAULT_WIDTH }))
            .collect();
    }

    // 3) Nothing known.
    vec![if missing > 0.0 { missing } else { DEFAULT_WIDTH }; 256]
}

fn lookup_metric(table: &[(&str, u16)], name: &str) -> Option<f32> {
    table
        .binary_search_by_key(&name, |(n, _)| *n)
        .ok()
        .map(|i| table[i].1 as f32)
}

/// Strip a subset prefix ("ABCDEF+Helvetica" → "Helvetica") and normalize a few
/// common aliases to their Core-14 name.
fn canonical_base(base: &str) -> String {
    let stripped = match base.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()) => rest,
        _ => base,
    };
    match stripped {
        "Arial" => "Helvetica".to_string(),
        "Arial-Bold" | "Arial,Bold" => "Helvetica-Bold".to_string(),
        "TimesNewRoman" | "Times" => "Times-Roman".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn empty_doc() -> Document {
        Document::with_version("1.5")
    }

    #[test]
    fn courier_is_monospace_600() {
        let doc = empty_doc();
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
        };
        let font = Font::from_dict(&doc, &dict);
        assert!(!font.is_cid);
        let (text, width) = font.decode(b'A');
        assert_eq!(text, Some("A"));
        assert_eq!(width, 600.0);
    }

    #[test]
    fn helvetica_uses_core14_metrics() {
        let doc = empty_doc();
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "Type1",
            "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        };
        let font = Font::from_dict(&doc, &dict);
        // WinAnsi 0x20 = space; Helvetica 'space' width = 278.
        assert_eq!(font.decode(b' '), (Some(" "), 278.0));
        // 'A' in Helvetica = 667.
        assert_eq!(font.decode(b'A'), (Some("A"), 667.0));
    }

    #[test]
    fn explicit_widths_array_wins() {
        let doc = empty_doc();
        // FirstChar 65 ('A'), Widths [111, 222] for 'A','B'.
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "Type1",
            "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
            "FirstChar" => 65, "LastChar" => 66,
            "Widths" => vec![Object::Integer(111), Object::Integer(222)],
        };
        let font = Font::from_dict(&doc, &dict);
        assert_eq!(font.decode(b'A').1, 111.0);
        assert_eq!(font.decode(b'B').1, 222.0);
    }

    #[test]
    fn type0_is_flagged_as_cid() {
        let doc = empty_doc();
        let dict = dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "X" };
        assert!(Font::from_dict(&doc, &dict).is_cid);
    }

    #[test]
    fn glyph_name_resolution() {
        assert_eq!(name_to_unicode("space"), Some(" ".to_string()));
        assert_eq!(name_to_unicode("A"), Some("A".to_string()));
        assert_eq!(name_to_unicode("uni0041"), Some("A".to_string()));
        assert_eq!(name_to_unicode("u1F600"), Some("😀".to_string()));
        assert_eq!(name_to_unicode(".notdef"), None);
        assert_eq!(name_to_unicode(""), None);
    }
}
