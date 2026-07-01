//! Structured error type.
//!
//! Errors are split into two paths, per the "graceful degradation" principle:
//!
//! - **Fatal** — the document cannot be parsed at all (bad container, wrong
//!   password, structurally broken). These return `Err(PdfmuseError)`.
//! - **Degradable** — a single page/object is damaged, a font lacks a CMap, a
//!   page needs OCR, etc. These do **not** error; they are recorded in
//!   [`crate::ir::Document::warnings`] and parsing continues.
//!
//! The core never `panic!`s on malformed input — every failure surfaces as one
//! of these two. Bindings map `PdfmuseError` onto each language's exception type.

use thiserror::Error;

/// Convenience alias used throughout the crate and by the public API.
pub type Result<T> = std::result::Result<T, PdfmuseError>;

/// A fatal parsing error. Non-fatal degradations use
/// [`crate::ir::Warning`] instead.
#[derive(Error, Debug)]
pub enum PdfmuseError {
    /// The bytes are not a recognized/supported document container.
    #[error("unrecognized or unsupported document format")]
    InvalidFormat,

    /// The format is recognized but not yet implemented (e.g. DOCX before M3).
    #[error("{0} is recognized but not yet supported")]
    Unsupported(String),

    /// The document is encrypted and no usable password was supplied.
    /// Password support lands in PER-50; the password is never logged.
    #[error("document is encrypted and requires a password")]
    EncryptedNoPassword,

    /// The document is structurally broken beyond recovery.
    #[error("malformed document: {0}")]
    Malformed(String),

    /// An I/O failure (for future `Read`-based entry points).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
