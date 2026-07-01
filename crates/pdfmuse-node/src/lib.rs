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
