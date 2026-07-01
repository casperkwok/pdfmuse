//! M0 end-to-end smoke test: the committed corpus PDF parses to non-empty text.
//!
//! This complements the unit test (which synthesizes a PDF in memory) by proving
//! the on-disk fixture in `tests/corpus/` round-trips through the public API.

use std::path::PathBuf;

use pdfmuse_core::ir::Block;

#[test]
fn corpus_hello_pdf_extracts_text() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/hello.pdf");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let doc = pdfmuse_core::parse(&data, None).expect("parse corpus PDF");
    assert_eq!(doc.source, pdfmuse_core::ir::SourceKind::Pdf);
    assert_eq!(doc.pages.len(), 1);

    let text: String = doc.pages[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(text.contains("Hello pdfmuse"), "extracted text was: {text:?}");
}
