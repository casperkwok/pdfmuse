//! Python binding (PyO3 + abi3).
//!
//! A thin shell: it forwards bytes to `pdfmuse-core` and returns the IR as a JSON
//! string. The pure-Python layer (`bindings/python/pdfmuse`) deserializes that
//! into typed dataclasses, so callers see `Document`/`Page`, not a raw string.

use pdfmuse_core::Format;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Parse `data` and return the unified IR serialized as a JSON string.
///
/// `fmt` forces a format ("pdf"/"docx"); `None` auto-detects from magic bytes.
#[pyfunction]
#[pyo3(signature = (data, fmt=None))]
fn parse_bytes(py: Python<'_>, data: &[u8], fmt: Option<String>) -> PyResult<String> {
    let format = match fmt.as_deref() {
        None => None,
        Some("pdf") => Some(Format::Pdf),
        Some("docx") => Some(Format::Docx),
        Some(other) => return Err(PyValueError::new_err(format!("unknown format: {other}"))),
    };

    // Take `data` as a borrowed `&[u8]` (zero-copy from Python `bytes`) then a
    // single memcpy to an owned buffer — extracting `Vec<u8>` directly makes PyO3
    // copy element-by-element, which is ~100ms on a multi-MB file. Owning the
    // bytes lets us release the GIL while the Rust parse runs.
    let owned = data.to_vec();
    let doc = py
        .allow_threads(move || pdfmuse_core::parse(&owned, format))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;

    serde_json::to_string(&doc).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_bytes, m)?)?;
    Ok(())
}
