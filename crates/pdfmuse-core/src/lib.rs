//! pdfmuse-core — deterministic PDF/DOCX parser core.
//!
//! The naive `parse()` lands in PER-33 and the self-written content-stream
//! interpreter (the real value) in PER-36. The unified IR — the data foundation
//! that every binding serializes byte-identically — lives in [`ir`].

pub mod error;
pub mod ir;
mod layout;
mod pdf;

pub use error::{PdfmuseError, Result};

/// Source-format hint for [`parse`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Docx,
}

/// Parse `data` into the unified [`ir::Document`].
///
/// `fmt` forces a format; `None` auto-detects from magic bytes. The core makes no
/// I/O assumptions — it only borrows `&[u8]`, so each binding feeds it bytes
/// however it likes (Python `bytes`, Node `Buffer`, WASM `Uint8Array`).
///
/// M0 uses lopdf's naive text extraction (one paragraph per page, no per-char
/// coordinates). PER-36 replaces the PDF path with the self-written content-stream
/// interpreter that fills [`ir::Page::chars`] with precise bboxes.
pub fn parse(data: &[u8], fmt: Option<Format>) -> Result<ir::Document> {
    parse_with_password(data, fmt, None)
}

/// Like [`parse`], but supplies a `password` for encrypted PDFs.
///
/// An encrypted document with no/incorrect password fails with
/// [`PdfmuseError::EncryptedNoPassword`]. The password is never logged or echoed.
pub fn parse_with_password(
    data: &[u8],
    fmt: Option<Format>,
    password: Option<&str>,
) -> Result<ir::Document> {
    match fmt.or_else(|| detect_format(data)) {
        Some(Format::Pdf) => {
            let mut doc = pdf::parse_pdf(data, password)?;
            // Geometric layout: chars → lines → paragraphs (reading order).
            for page in &mut doc.pages {
                layout::layout_page(page);
            }
            Ok(doc)
        }
        Some(Format::Docx) => Err(PdfmuseError::Unsupported("DOCX".to_string())),
        None => Err(PdfmuseError::InvalidFormat),
    }
}

/// Detect the container format from leading magic bytes.
fn detect_format(data: &[u8]) -> Option<Format> {
    if data.starts_with(b"PK\x03\x04") {
        return Some(Format::Docx); // ZIP container → OOXML (DOCX)
    }
    // Some PDFs carry leading junk before `%PDF-`; scan the first 1 KiB.
    let head = &data[..data.len().min(1024)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        return Some(Format::Pdf);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pdf_and_docx_magic() {
        assert_eq!(detect_format(b"%PDF-1.7\ntrailer"), Some(Format::Pdf));
        assert_eq!(detect_format(b"PK\x03\x04rest"), Some(Format::Docx));
        assert_eq!(detect_format(b"not a document"), None);
    }

    #[test]
    fn docx_is_recognized_but_unsupported() {
        assert!(matches!(
            parse(b"PK\x03\x04", None).unwrap_err(),
            PdfmuseError::Unsupported(_)
        ));
    }

    #[test]
    fn unknown_bytes_are_invalid_format() {
        assert!(matches!(
            parse(b"garbage", None).unwrap_err(),
            PdfmuseError::InvalidFormat
        ));
    }
}
