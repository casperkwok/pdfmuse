//! Content-stream interpreter — **the core value of pdfmuse**.
//!
//! Walks a page's content-stream operators, maintaining the graphics CTM stack
//! and the text state (text matrix, font, spacing), and emits [`Char`]s with
//! precise, normalized bounding boxes. This replaces the M0 naive `extract_text`
//! path. Operator tokenizing is delegated to lopdf (`Content::decode`); the
//! interpretation (matrices, placement, bboxes) is ours.
//!
//! M1 scope: simple (Type1/TrueType) Latin fonts. CID/Type0 fonts are flagged by
//! [`super::fonts::Font::is_cid`] and reported as a `MissingCMap` warning; real
//! CJK handling lands in M2. Glyph ink height is approximated as one em above the
//! baseline (font descriptor ascent/descent refinement is future work).

use std::collections::BTreeMap;

use lopdf::content::Content;
use lopdf::{Object, ObjectId};

use super::fonts::Font;
use super::objects::PdfDoc;
use crate::ir::{BBox, Char, FontRef, Warning, WarningKind};

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Interpret a page's content stream into positioned characters.
///
/// `page_height` (PDF points) flips PDF's bottom-left origin to the IR's
/// top-left, Y-down convention.
pub(crate) fn extract_page(
    pdf: &PdfDoc,
    page_id: ObjectId,
    page_index: u32,
    page_height: f32,
) -> (Vec<Char>, Vec<Warning>) {
    let mut chars = Vec::new();
    let mut warnings = Vec::new();

    let bytes = match pdf.content_bytes(page_id) {
        Ok(b) => b,
        Err(e) => {
            warnings.push(warn(page_index, format!("content stream unreadable: {e}")));
            return (chars, warnings);
        }
    };
    let ops = match Content::decode(&bytes) {
        Ok(c) => c.operations,
        Err(e) => {
            warnings.push(warn(page_index, format!("content stream failed to decode: {e}")));
            return (chars, warnings);
        }
    };

    let fonts = build_fonts(pdf, page_id);
    let mut st = TextState::default();
    let mut ctm_stack: Vec<[f32; 6]> = Vec::new();
    let mut warned_cid = false;

    for op in &ops {
        let a = &op.operands;
        match op.operator.as_str() {
            "q" => ctm_stack.push(st.ctm),
            "Q" => {
                if let Some(ctm) = ctm_stack.pop() {
                    st.ctm = ctm;
                }
            }
            "cm" => st.ctm = mul(mat6(a), st.ctm),
            "BT" => {
                st.tm = IDENTITY;
                st.tlm = IDENTITY;
            }
            "ET" => {}
            "Tf" => {
                st.font = name_bytes(a.first());
                st.font_size = num(a, 1);
            }
            "Td" => st.line_move(num(a, 0), num(a, 1)),
            "TD" => {
                st.leading = -num(a, 1);
                st.line_move(num(a, 0), num(a, 1));
            }
            "Tm" => {
                st.tlm = mat6(a);
                st.tm = st.tlm;
            }
            "T*" => st.line_move(0.0, -st.leading),
            "TL" => st.leading = num(a, 0),
            "Tc" => st.char_spacing = num(a, 0),
            "Tw" => st.word_spacing = num(a, 0),
            "Tz" => st.h_scale = num(a, 0) / 100.0,
            "Ts" => st.rise = num(a, 0),
            "Tj" => {
                if let Some(s) = string_bytes(a.first()) {
                    show(s, &mut st, &fonts, page_height, &mut chars, &mut warnings, page_index, &mut warned_cid);
                }
            }
            "'" => {
                st.line_move(0.0, -st.leading);
                if let Some(s) = string_bytes(a.first()) {
                    show(s, &mut st, &fonts, page_height, &mut chars, &mut warnings, page_index, &mut warned_cid);
                }
            }
            "\"" => {
                st.word_spacing = num(a, 0);
                st.char_spacing = num(a, 1);
                st.line_move(0.0, -st.leading);
                if let Some(s) = string_bytes(a.get(2)) {
                    show(s, &mut st, &fonts, page_height, &mut chars, &mut warnings, page_index, &mut warned_cid);
                }
            }
            "TJ" => {
                if let Some(Object::Array(items)) = a.first() {
                    for item in items {
                        match item {
                            Object::String(s, _) => show(s, &mut st, &fonts, page_height, &mut chars, &mut warnings, page_index, &mut warned_cid),
                            other => {
                                // Positive numbers move left (tighten); PDF: subtract /1000 * fs.
                                let adj = -number(other) / 1000.0 * st.font_size * st.h_scale;
                                st.tm = mul([1.0, 0.0, 0.0, 1.0, adj, 0.0], st.tm);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (chars, warnings)
}

/// Text state carried through the interpreter.
#[derive(Clone)]
struct TextState {
    ctm: [f32; 6],
    tm: [f32; 6],
    tlm: [f32; 6],
    font: Option<Vec<u8>>,
    font_size: f32,
    char_spacing: f32,
    word_spacing: f32,
    leading: f32,
    h_scale: f32,
    rise: f32,
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            ctm: IDENTITY,
            tm: IDENTITY,
            tlm: IDENTITY,
            font: None,
            font_size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            leading: 0.0,
            h_scale: 1.0,
            rise: 0.0,
        }
    }
}

impl TextState {
    /// `Td`: move to the start of the next line, offset (tx, ty) from the line matrix.
    fn line_move(&mut self, tx: f32, ty: f32) {
        self.tlm = mul([1.0, 0.0, 0.0, 1.0, tx, ty], self.tlm);
        self.tm = self.tlm;
    }
}

#[allow(clippy::too_many_arguments)]
fn show(
    bytes: &[u8],
    st: &mut TextState,
    fonts: &BTreeMap<Vec<u8>, Font>,
    page_height: f32,
    chars: &mut Vec<Char>,
    warnings: &mut Vec<Warning>,
    page_index: u32,
    warned_cid: &mut bool,
) {
    let font = match st.font.as_ref().and_then(|n| fonts.get(n)) {
        Some(f) => f,
        None => return, // no current font — skip
    };
    if font.is_cid {
        if !*warned_cid {
            warnings.push(Warning {
                page: Some(page_index),
                kind: WarningKind::MissingCMap,
                detail: format!("CID font '{}' not yet supported (M2)", font.base),
            });
            *warned_cid = true;
        }
        return;
    }

    for &code in bytes {
        let (text, w0) = font.decode(code);
        let w = w0 / 1000.0; // em fraction

        if let Some(t) = text {
            if !t.is_empty() {
                // Text rendering matrix: [fs*h 0 0 fs 0 rise] · Tm · CTM.
                let params = [st.font_size * st.h_scale, 0.0, 0.0, st.font_size, 0.0, st.rise];
                let trm = mul(params, mul(st.tm, st.ctm));
                // Glyph ink box in text space (em): baseline (0) to one em up.
                let corners = [
                    apply(&trm, 0.0, 0.0),
                    apply(&trm, w, 0.0),
                    apply(&trm, 0.0, 1.0),
                    apply(&trm, w, 1.0),
                ];
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (x, y_user) in corners {
                    let y = page_height - y_user; // flip to top-left origin
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
                chars.push(Char {
                    text: t.to_string(),
                    bbox: BBox { x0, y0, x1, y1 },
                    font: FontRef { name: font.base.clone() },
                    size: st.font_size,
                    color: None,
                });
            }
        }

        // Advance the text matrix.
        let mut adv = w * st.font_size + st.char_spacing;
        if code == b' ' {
            adv += st.word_spacing;
        }
        adv *= st.h_scale;
        st.tm = mul([1.0, 0.0, 0.0, 1.0, adv, 0.0], st.tm);
    }
}

/// Build `resource name -> Font` for a page.
fn build_fonts(pdf: &PdfDoc, page_id: ObjectId) -> BTreeMap<Vec<u8>, Font> {
    let mut map = BTreeMap::new();
    let Some(res) = pdf.page_resources(page_id) else {
        return map;
    };
    let Ok(fonts_obj) = res.get(b"Font") else {
        return map;
    };
    if let Object::Dictionary(fd) = deref(pdf, fonts_obj) {
        for (name, v) in fd.iter() {
            if let Object::Dictionary(font_dict) = deref(pdf, v) {
                map.insert(name.to_vec(), Font::from_dict(&pdf.inner, font_dict));
            }
        }
    }
    map
}

fn deref<'a>(pdf: &'a PdfDoc, o: &'a Object) -> &'a Object {
    pdf.inner.dereference(o).map(|(_, x)| x).unwrap_or(o)
}

// --- 2D affine helpers: matrix [a, b, c, d, e, f], row-vector convention ---
// x' = a*x + c*y + e ; y' = b*x + d*y + f

/// `m` then `n` (point · M · N).
fn mul(m: [f32; 6], n: [f32; 6]) -> [f32; 6] {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

fn apply(m: &[f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn mat6(ops: &[Object]) -> [f32; 6] {
    [num(ops, 0), num(ops, 1), num(ops, 2), num(ops, 3), num(ops, 4), num(ops, 5)]
}

fn num(ops: &[Object], i: usize) -> f32 {
    ops.get(i).map(number).unwrap_or(0.0)
}

fn number(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn name_bytes(o: Option<&Object>) -> Option<Vec<u8>> {
    match o {
        Some(Object::Name(n)) => Some(n.clone()),
        _ => None,
    }
}

fn string_bytes(o: Option<&Object>) -> Option<&[u8]> {
    match o {
        Some(Object::String(s, _)) => Some(s),
        _ => None,
    }
}

fn warn(page: u32, detail: String) -> Warning {
    Warning { page: Some(page), kind: WarningKind::MalformedObject, detail }
}
