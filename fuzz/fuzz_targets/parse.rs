//! Fuzz target: parse() must never panic on any input (principle 4).
//! Run: cargo +nightly fuzz run parse   (needs cargo-fuzz + nightly)
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Auto-detect and both forced formats — exercise every entry path.
    let _ = pdfmuse_core::parse(data, None);
    let _ = pdfmuse_core::parse(data, Some(pdfmuse_core::Format::Pdf));
    let _ = pdfmuse_core::parse(data, Some(pdfmuse_core::Format::Docx));
});
