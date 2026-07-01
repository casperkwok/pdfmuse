//! pdfmuse-core — deterministic PDF/DOCX parser core.
//!
//! The naive `parse()` lands in PER-33 and the self-written content-stream
//! interpreter (the real value) in PER-36. The unified IR — the data foundation
//! that every binding serializes byte-identically — lives in [`ir`].

pub mod ir;
