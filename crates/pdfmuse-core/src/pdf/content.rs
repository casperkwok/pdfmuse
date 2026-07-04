//! Content-stream interpreter — **the core value of pdfmuse**.
//!
//! Walks a page's content-stream operators, maintaining the graphics state (CTM,
//! line width) and text state (text matrix, font, spacing), and emits [`Char`]s
//! with precise, normalized bounding boxes plus the vector [`Rect`]/[`Rule`]
//! geometry that feeds table reconstruction. Operator tokenizing is delegated to
//! lopdf (`Content::decode`); the interpretation (matrices, placement, bboxes) is
//! ours.
//!
//! Fonts: simple (1-byte) and Type0/CID (2-byte) codes are both handled via
//! [`super::fonts::Font`] (`code_bytes` drives the read width). A CID font with
//! no `/ToUnicode` is flagged [`super::fonts::Font::unmapped_cid`] and reported
//! as a `MissingCMap` warning. Straight path segments and rectangles are
//! collected; Bézier curves advance the point but are not emitted.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use lopdf::{Dictionary, Object, ObjectId};
use unicode_normalization::UnicodeNormalization;

use super::content_lex::{Lexer, Operand, Token};
use super::fonts::Font;
use super::graphics;
use super::objects::PdfDoc;
use crate::ir::{BBox, Char, FontRef, Rect, Rule, Warning, WarningKind};

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// A CJK compatibility/radical codepoint whose glyph is really a normal ideograph.
/// Some fonts' `/ToUnicode` map here (e.g. Kangxi radical ⽬ U+2F6C for 目 U+76EE),
/// which silently breaks search/RAG. We NFKC just these blocks — deliberately not
/// the whole string, so ①②, full-width forms, and ligatures stay byte-exact.
fn is_cjk_compat(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2EFF   // CJK Radicals Supplement
        | 0x2F00..=0x2FDF // Kangxi Radicals
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0x2F800..=0x2FA1F // CJK Compatibility Ideographs Supplement
    )
}

/// Map CJK compatibility codepoints to their canonical ideograph; pass everything
/// else through untouched.
fn normalize_cjk_compat(s: &str) -> String {
    if !s.chars().any(is_cjk_compat) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if is_cjk_compat(c) {
            out.extend(c.to_string().nfkc());
        } else {
            out.push(c);
        }
    }
    out
}

/// Everything the interpreter extracts from one page.
pub(crate) struct PageContent {
    pub chars: Vec<Char>,
    pub rects: Vec<Rect>,
    pub rules: Vec<Rule>,
    pub warnings: Vec<Warning>,
}

/// A resolved form XObject: its decoded stream, fonts, resources and matrix. Cached
/// by `ObjectId` so a form invoked many times (e.g. a plot marker drawn thousands of
/// times) is decoded and has its fonts/CMaps built **once**, not per `Do`. Only the
/// (cheap) interpretation re-runs, with the caller's CTM. See PER-228.
struct CachedForm {
    bytes: Vec<u8>,
    fonts: BTreeMap<Vec<u8>, Font>,
    resources: Dictionary,
    matrix: [f32; 6],
}

/// Per-page cache of resolved form XObjects. `None` marks an id that resolved to a
/// non-form (image) XObject, so it isn't retried on every invocation either.
type FormCache = HashMap<ObjectId, Option<Rc<CachedForm>>>;

/// Interpret a page's content stream.
///
/// `page_height` (PDF points) flips PDF's bottom-left origin to the IR's
/// top-left, Y-down convention.
pub(crate) fn extract_page(
    pdf: &PdfDoc<'_>,
    page_id: ObjectId,
    page_index: u32,
    page_height: f32,
) -> PageContent {
    let mut out = PageContent { chars: Vec::new(), rects: Vec::new(), rules: Vec::new(), warnings: Vec::new() };

    let bytes = match pdf.content_bytes(page_id) {
        Ok(b) => b,
        Err(e) => {
            out.warnings.push(warn(page_index, format!("content stream unreadable: {e}")));
            return out;
        }
    };
    let resources = pdf.page_resources(page_id).unwrap_or_default();
    let fonts = build_fonts_from_resources(pdf, &resources);
    let mut warned_cid = false;
    let mut forms: FormCache = HashMap::new();
    run_stream(
        pdf, &bytes, &fonts, &resources, GraphicsState::default(),
        &mut out, page_height, page_index, &mut warned_cid, &mut forms, 0,
    );
    out
}

/// Interpret one content stream (page or form XObject), appending chars/rules to
/// `out`. Recurses into form XObjects invoked by `Do` (depth-guarded) so text drawn
/// inside forms — common in Canva / PDFium / design-tool PDFs — is not silently lost.
#[allow(clippy::too_many_arguments)]
fn run_stream(
    pdf: &PdfDoc<'_>,
    bytes: &[u8],
    fonts: &BTreeMap<Vec<u8>, Font>,
    resources: &Dictionary,
    mut st: GraphicsState,
    out: &mut PageContent,
    page_height: f32,
    page_index: u32,
    warned_cid: &mut bool,
    forms: &mut FormCache,
    depth: u8,
) {
    // Resolve this stream's /XObject subdictionary once (not per `Do`), so looking up
    // a form by name is a cheap dict lookup even when it's invoked thousands of times.
    let xobject_dict: Option<Dictionary> = resources
        .get(b"XObject")
        .ok()
        .and_then(|o| pdf.resolve(o))
        .and_then(|o| o.as_dict().ok().cloned());
    let mut stack: Vec<GraphicsState> = Vec::new();
    let mut path = Path::default();
    // Path geometry is buffered until the painting op: only *stroked* paths become
    // table borders. Filled rectangles are decorative (backgrounds/highlights), not
    // grid lines, so they must not fool ruled-table detection.
    let mut pending_rects: Vec<Rect> = Vec::new();
    let mut pending_rules: Vec<Rule> = Vec::new();
    let mut lex = Lexer::new(bytes);
    let mut operands: Vec<Operand> = Vec::new();
    while let Some(tok) = lex.next() {
        let kw = match tok {
            Token::Operand(o) => {
                operands.push(o);
                continue;
            }
            Token::Operator(kw) => kw,
        };
        let a: &[Operand] = &operands;
        match std::str::from_utf8(kw).unwrap_or("") {
            // --- graphics state ---
            "q" => stack.push(st.clone()),
            "Q" => {
                if let Some(s) = stack.pop() {
                    st = s;
                }
            }
            "cm" => st.ctm = mul(mat6(a), st.ctm),
            "w" => st.line_width = num(a, 0),

            // --- path construction ---
            "m" => {
                let p = (num(a, 0), num(a, 1));
                path.cur = Some(p);
                path.start = Some(p);
            }
            "l" => {
                let p = (num(a, 0), num(a, 1));
                if let Some(prev) = path.cur {
                    if let Some(r) = graphics::make_rule(apply(&st.ctm, prev.0, prev.1), apply(&st.ctm, p.0, p.1), st.line_width, page_height) {
                        pending_rules.push(r);
                    }
                }
                path.cur = Some(p);
            }
            "re" => {
                let (x, y, w, h) = (num(a, 0), num(a, 1), num(a, 2), num(a, 3));
                let corners = [
                    apply(&st.ctm, x, y),
                    apply(&st.ctm, x + w, y),
                    apply(&st.ctm, x, y + h),
                    apply(&st.ctm, x + w, y + h),
                ];
                pending_rects.push(graphics::make_rect(corners, page_height));
                path.cur = Some((x, y));
                path.start = Some((x, y));
            }
            // Bézier curves: advance the current point, don't emit (M1).
            "c" => path.cur = Some((num(a, 4), num(a, 5))),
            "v" | "y" => path.cur = Some((num(a, 2), num(a, 3))),
            "h" => {
                if let (Some(cur), Some(start)) = (path.cur, path.start) {
                    if let Some(r) = graphics::make_rule(apply(&st.ctm, cur.0, cur.1), apply(&st.ctm, start.0, start.1), st.line_width, page_height) {
                        pending_rules.push(r);
                    }
                }
                path.cur = path.start;
            }
            // Painting ops end the path. Stroked paths (S/B family) are real borders
            // → commit them; fill-only (f) or no-paint (n) paths are discarded.
            "S" | "s" | "B" | "B*" | "b" | "b*" => {
                out.rects.append(&mut pending_rects);
                out.rules.append(&mut pending_rules);
                path = Path::default();
            }
            "f" | "F" | "f*" | "n" => {
                pending_rects.clear();
                pending_rules.clear();
                path = Path::default();
            }

            // --- text object ---
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
            "Tj" => show_operand(a.first(), &mut st, fonts, page_height, out, page_index, warned_cid),
            "'" => {
                st.line_move(0.0, -st.leading);
                show_operand(a.first(), &mut st, fonts, page_height, out, page_index, warned_cid);
            }
            "\"" => {
                st.word_spacing = num(a, 0);
                st.char_spacing = num(a, 1);
                st.line_move(0.0, -st.leading);
                show_operand(a.get(2), &mut st, fonts, page_height, out, page_index, warned_cid);
            }
            "TJ" => {
                if let Some(Operand::Array(items)) = a.first() {
                    for item in items {
                        match item {
                            Operand::Str(s) => show(s, &mut st, fonts, page_height, out, page_index, warned_cid),
                            Operand::Num(n) => {
                                // Positive numbers move left (tighten): subtract /1000 * fs.
                                let adj = -n / 1000.0 * st.font_size * st.h_scale;
                                st.tm = mul([1.0, 0.0, 0.0, 1.0, adj, 0.0], st.tm);
                            }
                            _ => {}
                        }
                    }
                }
            }
            // Inline image: skip the BI..ID..EI binary payload so it is not tokenized.
            "BI" => lex.skip_inline_image(),
            // Form XObject: recurse into its content stream so text drawn inside a
            // form (Canva/PDFium/design tools wrap the whole page this way) is
            // extracted instead of silently dropped.
            "Do" if depth < 12 => {
                // Resolve → decode → build fonts once per unique form, then reuse.
                if let Some(id) = name_bytes(a.first())
                    .as_deref()
                    .and_then(|n| xobject_dict.as_ref()?.get(n).ok()?.as_reference().ok())
                {
                    let cached = forms
                        .entry(id)
                        .or_insert_with(|| {
                            resolve_form_stream(pdf, id).map(|(bytes, fres, matrix)| {
                                let resources = fres.unwrap_or_else(|| resources.clone());
                                let fonts = build_fonts_from_resources(pdf, &resources);
                                Rc::new(CachedForm { bytes, fonts, resources, matrix })
                            })
                        })
                        .clone();
                    if let Some(cf) = cached {
                        let mut child = st.clone();
                        child.ctm = mul(cf.matrix, st.ctm);
                        run_stream(
                            pdf, &cf.bytes, &cf.fonts, &cf.resources, child,
                            out, page_height, page_index, warned_cid, forms, depth + 1,
                        );
                    }
                }
            }
            _ => {}
        }
        operands.clear();
    }
}

/// Combined graphics + text state carried through the interpreter.
#[derive(Clone)]
struct GraphicsState {
    ctm: [f32; 6],
    line_width: f32,
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

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState {
            ctm: IDENTITY,
            line_width: 1.0,
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

impl GraphicsState {
    /// `Td`: move to the start of the next line, offset (tx, ty) from the line matrix.
    fn line_move(&mut self, tx: f32, ty: f32) {
        self.tlm = mul([1.0, 0.0, 0.0, 1.0, tx, ty], self.tlm);
        self.tm = self.tlm;
    }
}

/// Current path-construction state.
#[derive(Default)]
struct Path {
    cur: Option<(f32, f32)>,
    start: Option<(f32, f32)>,
}

fn show_operand(
    o: Option<&Operand>,
    st: &mut GraphicsState,
    fonts: &BTreeMap<Vec<u8>, Font>,
    page_height: f32,
    out: &mut PageContent,
    page_index: u32,
    warned_cid: &mut bool,
) {
    if let Some(Operand::Str(s)) = o {
        show(s, st, fonts, page_height, out, page_index, warned_cid);
    }
}

fn show(
    bytes: &[u8],
    st: &mut GraphicsState,
    fonts: &BTreeMap<Vec<u8>, Font>,
    page_height: f32,
    out: &mut PageContent,
    page_index: u32,
    warned_cid: &mut bool,
) {
    let font = match st.font.as_ref().and_then(|n| fonts.get(n)) {
        Some(f) => f,
        None => return, // no current font — skip
    };
    if font.unmapped_cid {
        if !*warned_cid {
            out.warnings.push(Warning {
                page: Some(page_index),
                kind: WarningKind::MissingCMap,
                detail: format!("CID font '{}' has no ToUnicode; text not recoverable", font.base),
            });
            *warned_cid = true;
        }
        return;
    }

    // Simple fonts read one byte per code; Type0/CID read two (big-endian).
    let step = font.code_bytes.max(1);
    let mut k = 0;
    while k < bytes.len() {
        let code = if step == 2 && k + 1 < bytes.len() {
            ((bytes[k] as u32) << 8) | bytes[k + 1] as u32
        } else {
            bytes[k] as u32
        };
        let (text, w0) = font.decode(code);
        let w = w0 / 1000.0; // em fraction

        if let Some(t) = text {
            if !t.is_empty() {
                // Text rendering matrix: [fs*h 0 0 fs 0 rise] · Tm · CTM.
                let params = [st.font_size * st.h_scale, 0.0, 0.0, st.font_size, 0.0, st.rise];
                let trm = mul(params, mul(st.tm, st.ctm));
                // Effective on-page size = length of the transformed y-basis. Some
                // PDFs set a large Tf size and scale it down via the text/CTM
                // matrix; the raw Tf value would be meaningless (and, fed to the
                // baseline tolerance, collapses every line into one).
                let eff_size = (trm[2] * trm[2] + trm[3] * trm[3]).sqrt();
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
                out.chars.push(Char {
                    text: normalize_cjk_compat(t),
                    bbox: BBox { x0, y0, x1, y1 },
                    font: FontRef { name: font.base.clone() },
                    size: eff_size,
                    color: None,
                });
            }
        }

        // Advance the text matrix. Word spacing applies only to single-byte code 32.
        let mut adv = w * st.font_size + st.char_spacing;
        if step == 1 && code == 32 {
            adv += st.word_spacing;
        }
        adv *= st.h_scale;
        st.tm = mul([1.0, 0.0, 0.0, 1.0, adv, 0.0], st.tm);

        k += step;
    }
}

/// Build `resource name -> Font` for a page.
fn build_fonts_from_resources(pdf: &PdfDoc<'_>, res: &Dictionary) -> BTreeMap<Vec<u8>, Font> {
    let mut map = BTreeMap::new();
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

/// Resolve a form XObject by id to `(decoded content, its /Resources, its /Matrix)`.
/// Returns `None` for image XObjects — they carry no extractable text.
fn resolve_form_stream(pdf: &PdfDoc<'_>, id: ObjectId) -> Option<(Vec<u8>, Option<Dictionary>, [f32; 6])> {
    let stream_obj = pdf.resolve(&Object::Reference(id))?;
    let Object::Stream(s) = &stream_obj else {
        return None;
    };
    if s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Form".as_ref()) {
        return None;
    }
    // Uncompressed streams: `decompressed_content()` errors when there is no
    // `/Filter`, so read the raw bytes directly in that case.
    let bytes = if s.dict.get(b"Filter").is_ok() {
        s.decompressed_content().ok()?
    } else {
        s.content.clone()
    };
    let matrix = s
        .dict
        .get(b"Matrix")
        .ok()
        .and_then(|o| pdf.resolve(o))
        .and_then(|o| o.as_array().ok().cloned())
        .filter(|a| a.len() == 6)
        .map(|a| {
            let mut m = IDENTITY;
            for (slot, v) in m.iter_mut().zip(&a) {
                *slot = obj_num(v);
            }
            m
        })
        .unwrap_or(IDENTITY);
    let res = s
        .dict
        .get(b"Resources")
        .ok()
        .and_then(|o| pdf.resolve(o))
        .and_then(|o| o.as_dict().ok().cloned());
    Some((bytes, res, matrix))
}

fn obj_num(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn deref<'a>(pdf: &'a PdfDoc<'_>, o: &'a Object) -> &'a Object {
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

fn mat6(ops: &[Operand]) -> [f32; 6] {
    [num(ops, 0), num(ops, 1), num(ops, 2), num(ops, 3), num(ops, 4), num(ops, 5)]
}

fn num(ops: &[Operand], i: usize) -> f32 {
    match ops.get(i) {
        Some(Operand::Num(n)) => *n,
        _ => 0.0,
    }
}

fn name_bytes(o: Option<&Operand>) -> Option<Vec<u8>> {
    match o {
        Some(Operand::Name(n)) => Some(n.clone()),
        _ => None,
    }
}

fn warn(page: u32, detail: String) -> Warning {
    Warning { page: Some(page), kind: WarningKind::MalformedObject, detail }
}

#[cfg(test)]
mod tests {
    use super::normalize_cjk_compat;

    #[test]
    fn normalizes_cjk_compat_but_leaves_the_rest() {
        // Kangxi radicals ⽬ U+2F6C / ⾼ U+2F98 / ⼩ U+2F29 → 目 高 小.
        assert_eq!(normalize_cjk_compat("题⽬⾼⼩"), "题目高小");
        // Circled numbers, ASCII, and normal CJK must be byte-exact.
        assert_eq!(normalize_cjk_compat("①②abc中"), "①②abc中");
        // No compat chars → same string, no allocation surprises.
        assert_eq!(normalize_cjk_compat("hello"), "hello");
    }
}
