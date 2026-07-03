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

fn parse_format(fmt: Option<&str>) -> PyResult<Option<Format>> {
    match fmt {
        None => Ok(None),
        Some("pdf") => Ok(Some(Format::Pdf)),
        Some("docx") => Ok(Some(Format::Docx)),
        Some(other) => Err(PyValueError::new_err(format!("unknown format: {other}"))),
    }
}

/// Parse `data` and return plain reading-order text. Avoids materializing the full
/// IR on the Python side (no `json.loads`), so the text path keeps the Rust speed.
#[pyfunction]
#[pyo3(signature = (data, fmt=None, drop_boilerplate=false))]
fn text_bytes(py: Python<'_>, data: &[u8], fmt: Option<String>, drop_boilerplate: bool) -> PyResult<String> {
    let format = parse_format(fmt.as_deref())?;
    let owned = data.to_vec();
    let mut doc = py
        .allow_threads(move || pdfmuse_core::parse(&owned, format))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    if drop_boilerplate {
        pdfmuse_core::remove_boilerplate(&mut doc);
    }
    Ok(pdfmuse_core::to_text(&doc))
}

/// Parse `data` and return structured Markdown (headings + tables) — a single Rust
/// call returning one string, no per-object materialization on the Python side.
/// `drop_boilerplate` strips running headers/footers first.
#[pyfunction]
#[pyo3(signature = (data, fmt=None, drop_boilerplate=false))]
fn markdown_bytes(py: Python<'_>, data: &[u8], fmt: Option<String>, drop_boilerplate: bool) -> PyResult<String> {
    let format = parse_format(fmt.as_deref())?;
    let owned = data.to_vec();
    let mut doc = py
        .allow_threads(move || pdfmuse_core::parse(&owned, format))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    if drop_boilerplate {
        pdfmuse_core::remove_boilerplate(&mut doc);
    }
    Ok(pdfmuse_core::to_markdown(&doc))
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(text_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(markdown_bytes, m)?)?;
    Ok(())
}
