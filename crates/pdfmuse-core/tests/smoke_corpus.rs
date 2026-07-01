//! M0 end-to-end smoke test: the committed corpus PDF parses to non-empty text.
//!
//! This complements the unit test (which synthesizes a PDF in memory) by proving
//! the on-disk fixture in `tests/corpus/` round-trips through the public API.

use std::path::PathBuf;

#[test]
fn corpus_hello_pdf_extracts_positioned_text() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/hello.pdf");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let doc = pdfmuse_core::parse(&data, None).expect("parse corpus PDF");
    assert_eq!(doc.source, pdfmuse_core::ir::SourceKind::Pdf);
    assert_eq!(doc.pages.len(), 1);

    let chars = &doc.pages[0].chars;
    let text: String = chars.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(text, "Hello pdfmuse");
    // Every char carries a positive-area bbox in normalized coordinates.
    assert!(chars.iter().all(|c| c.bbox.x1 > c.bbox.x0 && c.bbox.y1 > c.bbox.y0));
}
