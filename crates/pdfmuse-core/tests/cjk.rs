//! CJK correctness regression net (M2).
//!
//! A Type0/CID font with a ToUnicode CMap must yield real Chinese characters —
//! not CIDs or mojibake. This is the failure mode that sinks many Rust extractors
//! on Chinese PDFs; pinning it here guards against regressions.

use std::path::PathBuf;

fn corpus(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn type0_cjk_extracts_chinese() {
    let doc = pdfmuse_core::parse(&corpus("cjk.pdf"), None).expect("parse cjk.pdf");
    assert!(doc.warnings.is_empty(), "unexpected warnings: {:?}", doc.warnings);

    let chars = &doc.pages[0].chars;
    let text: String = chars.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(text, "中文"); // 2-byte Identity-H codes 0x0001/0x0002 via ToUnicode
    assert_eq!(chars.len(), 2);
    // Each CJK glyph carries a positive-area bbox in normalized coordinates.
    assert!(chars.iter().all(|c| c.bbox.x1 > c.bbox.x0 && c.bbox.y1 > c.bbox.y0));
}
