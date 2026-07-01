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
fn parse_bytes(py: Python<'_>, data: Vec<u8>, fmt: Option<String>) -> PyResult<String> {
    let format = match fmt.as_deref() {
        None => None,
        Some("pdf") => Some(Format::Pdf),
        Some("docx") => Some(Format::Docx),
        Some(other) => return Err(PyValueError::new_err(format!("unknown format: {other}"))),
    };

    // Release the GIL so the Rust parse runs without blocking other Python
    // threads. `data` is owned, so it is safe to move across the GIL boundary.
    let doc = py
        .allow_threads(move || pdfmuse_core::parse(&data, format))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;

    serde_json::to_string(&doc).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_bytes, m)?)?;
    Ok(())
}
