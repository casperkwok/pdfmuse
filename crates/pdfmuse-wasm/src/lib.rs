//! WASM binding (wasm-bindgen).
//!
//! A thin shell: it forwards bytes to `pdfmuse-core` and returns the IR as a JSON
//! string. The browser-side JS layer deserializes that into typed objects, so
//! callers see `Document`/`Page`, not a raw string.
//!
//! Positioning: this build targets **browser-side lightweight parsing of digital
//! PDFs** — the deterministic, ML-free core runs entirely client-side, no bytes
//! leave the page. Scanned pages carry no extractable text; the core records a
//! `NeedsOcr` warning rather than guessing, and the front-end decides whether to
//! ship those pages to a server-side OCR backend. OCR itself is out of scope here.

use pdfmuse_core::Format;
use wasm_bindgen::prelude::*;

/// Parse `data` and return the unified IR serialized as a JSON string.
///
/// `fmt` forces a format ("pdf"/"docx"); `None`/`undefined` auto-detects from
/// magic bytes. Core errors and an unknown `fmt` surface as a JS exception.
#[wasm_bindgen]
pub fn parse(data: &[u8], fmt: Option<String>) -> Result<String, JsValue> {
    let format = match fmt.as_deref() {
        None => None,
        Some("pdf") => Some(Format::Pdf),
        Some("docx") => Some(Format::Docx),
        Some(other) => return Err(JsValue::from_str(&format!("unknown format: {other}"))),
    };

    let doc = pdfmuse_core::parse(data, format).map_err(|e| JsValue::from_str(&e.to_string()))?;

    serde_json::to_string(&doc).map_err(|e| JsValue::from_str(&e.to_string()))
}
