//! Pluggable vision backend — the ML boundary.
//!
//! The core is deterministic and has **zero ML dependencies**. Everything that
//! needs a model — OCR for scanned pages, structure recognition for borderless
//! tables — goes through this trait, not the core. The default [`NoopBackend`]
//! does no inference: a scanned page is surfaced as a `NeedsOcr` warning and left
//! for a real backend (an ONNX/Tesseract crate, or a Python-side cloud OCR) to
//! fill in. Reference backends live in separate optional crates so the core never
//! links a model runtime.

use crate::ir::{Cell, Char};

/// A backend that can recover content the deterministic core cannot.
pub trait VisionBackend: Send + Sync {
    /// OCR a rasterized page (PNG at `dpi`) into positioned characters.
    fn ocr_page(&self, page_png: &[u8], dpi: u32) -> Result<Vec<Char>, BackendError>;

    /// Recognize the cell structure of a borderless-table region (PNG).
    fn detect_table(&self, region_png: &[u8]) -> Result<Vec<Vec<Cell>>, BackendError>;
}

/// The default backend: no model inference. Scanned pages surface as warnings,
/// keeping the core free of any ML runtime.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopBackend;

impl VisionBackend for NoopBackend {
    fn ocr_page(&self, _page_png: &[u8], _dpi: u32) -> Result<Vec<Char>, BackendError> {
        Err(BackendError::Unsupported)
    }

    fn detect_table(&self, _region_png: &[u8]) -> Result<Vec<Vec<Cell>>, BackendError> {
        Err(BackendError::Unsupported)
    }
}

/// An error from a vision backend.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// This backend does not implement the requested operation (e.g. [`NoopBackend`]).
    #[error("operation not supported by this backend")]
    Unsupported,
    /// The backend failed at runtime.
    #[error("backend failure: {0}")]
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_backend_does_no_inference() {
        let b = NoopBackend;
        assert!(matches!(b.ocr_page(&[], 300), Err(BackendError::Unsupported)));
        assert!(matches!(b.detect_table(&[]), Err(BackendError::Unsupported)));
    }
}
