//! Font handling.
//!
//! **Simple** (Type1/TrueType) fonts map each byte code to a Unicode string (base
//! encoding + `Differences` + the Adobe Glyph List, overridden by `/ToUnicode`)
//! and to an advance width (`/Widths`, else Core-14 metrics, else a fallback).
//!
//! **CID / Type0** fonts (the CJK case) use 2-byte codes: text comes from the
//! `/ToUnicode` CMap and widths from the descendant CIDFont's `/W` array. A CID
//! font with no `/ToUnicode` is flagged [`Font::unmapped_cid`] so the interpreter
//! can warn instead of guessing.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Object};

use super::tables::{agl::AGL, encodings, metrics};

/// Fallback advance width (1/1000 em) when nothing better is known.
const DEFAULT_WIDTH: f32 = 500.0;

pub(crate) struct Font {
    /// Bytes consumed per character code: 1 for simple fonts, 2 for Type0/CID.
    pub(crate) code_bytes: usize,
    kind: FontKind,
    /// BaseFont name, used to label chars in the IR.
    pub(crate) base: String,
    /// A CID font we couldn't map to text (no `/ToUnicode`) → the interpreter warns.
    pub(crate) unmapped_cid: bool,
}

enum FontKind {
    /// One glyph per byte code (0..=255).
    Simple(Vec<Glyph>),
    /// CID font: codes map to text via ToUnicode and to widths via `/W`.
    Cid { to_unicode: BTreeMap<u32, String>, widths: BTreeMap<u32, f32>, default_width: f32 },
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
            return cid_font(doc, dict, base);
        }

        let names = encoding_names(doc, dict, &base);
        let widths = resolve_widths(doc, dict, &base, &names);
        // Widths are stored in 1/1000 em. Type1/TrueType /Widths already use that
        // unit (factor 1.0). Type3 fonts express widths in their /FontMatrix glyph
        // space, so convert with FontMatrix x-scale × 1000 — otherwise the widths
        // are ~1000× too small and every glyph's advance collapses to zero (all
        // text stacks at one x). Normal fonts multiply by exactly 1.0 → unchanged.
        let wfactor = type3_width_factor(doc, dict);
        // A /ToUnicode CMap, when present, is the authoritative code → text map.
        let to_unicode = to_unicode_map(doc, dict);

        let glyphs = (0..256)
            .map(|c| Glyph {
                text: to_unicode
                    .as_ref()
                    .and_then(|m| m.get(&(c as u32)).cloned())
                    .or_else(|| name_to_unicode(&names[c])),
                width: widths[c] * wfactor,
            })
            .collect();
        Font { code_bytes: 1, kind: FontKind::Simple(glyphs), base, unmapped_cid: false }
    }

    /// `(unicode text, advance width in 1/1000 em)` for a character code.
    pub(crate) fn decode(&self, code: u32) -> (Option<&str>, f32) {
        match &self.kind {
            FontKind::Simple(glyphs) => {
                let g = &glyphs[(code & 0xFF) as usize];
                (g.text.as_deref(), g.width)
            }
            FontKind::Cid { to_unicode, widths, default_width } => (
                to_unicode.get(&code).map(String::as_str),
                *widths.get(&code).unwrap_or(default_width),
            ),
        }
    }
}

/// Build a Type0/CID font. M2 handles the common case: 2-byte codes (Identity or
/// a predefined CMap) with a `/ToUnicode` text map and `/W` CID widths. Since
/// ToUnicode is keyed by the shown code, text is correct regardless of the CID
/// mapping; widths assume CID == code (exact for Identity encodings).
fn cid_font(doc: &Document, dict: &Dictionary, base: String) -> Font {
    let to_unicode = to_unicode_map(doc, dict).unwrap_or_default();
    let (widths, default_width) = cid_widths(doc, dict);
    let unmapped_cid = to_unicode.is_empty();
    Font { code_bytes: 2, kind: FontKind::Cid { to_unicode, widths, default_width }, base, unmapped_cid }
}

/// CID widths from the descendant CIDFont's `/W` array (+ `/DW` default = 1000).
fn cid_widths(doc: &Document, dict: &Dictionary) -> (BTreeMap<u32, f32>, f32) {
    let mut widths = BTreeMap::new();
    let mut default_width = 1000.0;
    let Some(desc) = descendant(doc, dict) else {
        return (widths, default_width);
    };
    if let Ok(dw) = desc.get(b"DW") {
        default_width = number(deref(doc, dw));
    }
    if let Ok(Object::Array(items)) = desc.get(b"W").map(|o| deref(doc, o)) {
        parse_w(doc, items, &mut widths);
    }
    (widths, default_width)
}

fn descendant(doc: &Document, dict: &Dictionary) -> Option<Dictionary> {
    let arr = deref(doc, dict.get(b"DescendantFonts").ok()?).as_array().ok()?;
    deref(doc, arr.first()?).as_dict().ok().cloned()
}

/// Parse `/W`: `c [w1 w2 …]` (consecutive CIDs from `c`) or `c_first c_last w`.
fn parse_w(doc: &Document, items: &[Object], out: &mut BTreeMap<u32, f32>) {
    let mut i = 0;
    while i < items.len() {
        let c = number(deref(doc, &items[i])) as u32;
        match items.get(i + 1).map(|o| deref(doc, o)) {
            Some(Object::Array(ws)) => {
                for (k, w) in ws.iter().enumerate() {
                    out.insert(c + k as u32, number(deref(doc, w)));
                }
                i += 2;
            }
            Some(second) => {
                let c_last = number(second) as u32;
                let w = items.get(i + 2).map(|o| number(deref(doc, o))).unwrap_or(0.0);
                for cid in c..=c_last {
                    out.insert(cid, w);
                }
                i += 3;
            }
            None => break,
        }
    }
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|(_, x)| x).unwrap_or(o)
}

/// Parse a font's `/ToUnicode` CMap into a `code -> text` map, if present.
fn to_unicode_map(doc: &Document, dict: &Dictionary) -> Option<std::collections::BTreeMap<u32, String>> {
    let obj = dict.get(b"ToUnicode").ok()?;
    if let Object::Stream(s) = deref(doc, obj) {
        // Handle both filtered (usual) and raw uncompressed ToUnicode streams.
        let content = if s.dict.get(b"Filter").is_ok() {
            s.decompressed_content().ok()?
        } else {
            s.content.clone()
        };
        Some(super::cmap::parse_to_unicode(&content))
    } else {
        None
    }
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
            Object::Name(name) if code < 256 => {
                names[code] = String::from_utf8_lossy(name).into_owned();
                code += 1;
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
/// Factor to bring a simple font's `/Widths` into the 1/1000-em unit the rest of
/// the pipeline assumes. Type1/TrueType have no `/FontMatrix` (already 1/1000 →
/// `1.0`); Type3 declares one, and its widths are in that glyph space, so
/// `FontMatrix[0] × 1000` converts them. Returning exactly `1.0` for the common
/// case keeps output byte-identical.
fn type3_width_factor(doc: &Document, dict: &Dictionary) -> f32 {
    let Ok(m) = dict.get(b"FontMatrix") else {
        return 1.0;
    };
    let Ok(arr) = deref(doc, m).as_array() else {
        return 1.0;
    };
    match arr.first().map(number) {
        Some(sx) if sx != 0.0 => sx * 1000.0,
        _ => 1.0,
    }
}

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
    use lopdf::{dictionary, Document, Object, Stream};

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
        assert_eq!(font.code_bytes, 1);
        let (text, width) = font.decode(u32::from(b'A'));
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
        assert_eq!(font.decode(u32::from(b' ')), (Some(" "), 278.0));
        // 'A' in Helvetica = 667.
        assert_eq!(font.decode(u32::from(b'A')), (Some("A"), 667.0));
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
        assert_eq!(font.decode(u32::from(b'A')).1, 111.0);
        assert_eq!(font.decode(u32::from(b'B')).1, 222.0);
    }

    #[test]
    fn type0_without_tounicode_is_unmapped() {
        let doc = empty_doc();
        let dict = dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "X" };
        let f = Font::from_dict(&doc, &dict);
        assert_eq!(f.code_bytes, 2);
        assert!(f.unmapped_cid);
    }

    #[test]
    fn type0_cid_decodes_via_tounicode_and_w() {
        let mut doc = empty_doc();
        // 2-byte codes 0x0001 → 中, 0x0002 → 文.
        let cmap = b"beginbfchar\n<0001> <4E2D>\n<0002> <6587>\nendbfchar".to_vec();
        let tu = doc.add_object(Stream::new(lopdf::Dictionary::new(), cmap));
        // Descendant CIDFont: W = [1 [900 950]] → CID 1 = 900, CID 2 = 950.
        let cidfont = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "X",
            "DW" => 1000,
            "W" => vec![Object::Integer(1), Object::Array(vec![Object::Integer(900), Object::Integer(950)])],
        });
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "X",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Reference(cidfont)],
            "ToUnicode" => tu,
        };
        let f = Font::from_dict(&doc, &dict);
        assert_eq!(f.code_bytes, 2);
        assert!(!f.unmapped_cid);
        assert_eq!(f.decode(0x0001), (Some("\u{4E2D}"), 900.0)); // 中
        assert_eq!(f.decode(0x0002), (Some("\u{6587}"), 950.0)); // 文
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

    #[test]
    fn to_unicode_overrides_encoding() {
        // A ToUnicode CMap remapping code 0x41 → 'Z' must win over WinAnsi's 'A'.
        let mut doc = empty_doc();
        let cmap = b"1 beginbfchar\n<41> <005A>\nendbfchar".to_vec();
        let tu_id = doc.add_object(Stream::new(lopdf::Dictionary::new(), cmap));
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "Type1",
            "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
            "ToUnicode" => tu_id,
        };
        let font = Font::from_dict(&doc, &dict);
        assert_eq!(font.decode(u32::from(b'A')).0, Some("Z"));
    }
}
