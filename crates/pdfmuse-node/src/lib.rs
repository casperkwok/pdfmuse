//! Node.js binding (napi-rs).
//!
//! A thin shell: it forwards bytes to `pdfmuse-core` and returns the IR as a JSON
//! string. The TypeScript layer (`bindings/node`) deserializes that into typed
//! objects, so callers see `Document`/`Page`, not a raw string.

#[macro_use]
extern crate napi_derive;

use napi::bindgen_prelude::Buffer;
use pdfmuse_core::Format;

/// Parse `data` and return the unified IR serialized as a JSON string.
///
/// `fmt` forces a format ("pdf"/"docx"); `None`/`undefined` auto-detects from
/// magic bytes.
///
/// `js_name` keeps the snake_case name on the JS side (napi camelCases by
/// default), mirroring the Python `_native.parse_bytes` shape.
#[napi(js_name = "parse_buffer")]
pub fn parse_buffer(data: Buffer, fmt: Option<String>) -> napi::Result<String> {
    let format = match fmt.as_deref() {
        None => None,
        Some("pdf") => Some(Format::Pdf),
        Some("docx") => Some(Format::Docx),
        Some(other) => {
            return Err(napi::Error::from_reason(format!("unknown format: {other}")));
        }
    };

    let doc = pdfmuse_core::parse(&data, format).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    serde_json::to_string(&doc).map_err(|e| napi::Error::from_reason(e.to_string()))
}

fn to_format(fmt: Option<&str>) -> napi::Result<Option<Format>> {
    match fmt {
        None => Ok(None),
        Some("pdf") => Ok(Some(Format::Pdf)),
        Some("docx") => Ok(Some(Format::Docx)),
        Some(other) => Err(napi::Error::from_reason(format!("unknown format: {other}"))),
    }
}

/// Parse `data` and return plain reading-order text. One Rust call returning a
/// string — no full-IR JSON to parse on the JS side, so the text path stays fast.
#[napi(js_name = "text_buffer")]
pub fn text_buffer(data: Buffer, fmt: Option<String>) -> napi::Result<String> {
    let format = to_format(fmt.as_deref())?;
    let doc = pdfmuse_core::parse(&data, format).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(pdfmuse_core::to_text(&doc))
}

/// Parse `data` and return structured Markdown (headings + tables) as one string.
#[napi(js_name = "markdown_buffer")]
pub fn markdown_buffer(data: Buffer, fmt: Option<String>) -> napi::Result<String> {
    let format = to_format(fmt.as_deref())?;
    let doc = pdfmuse_core::parse(&data, format).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(pdfmuse_core::to_markdown(&doc))
}
