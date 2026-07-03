//! Regression for PER-167: PDF heading detection (font-size clustering + numbering)
//! must populate `heading_level`, so Markdown `#` levels and RAG `heading_path` work
//! for PDF — not just DOCX.

use pdfmuse_core::ir::{Block, Document};

fn fixture() -> Document {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/headings.pdf");
    let data = std::fs::read(path).expect("fixture");
    pdfmuse_core::parse(&data, None).expect("parse")
}

#[test]
fn markdown_marks_headings_by_size_and_numbering() {
    let md = pdfmuse_core::to_markdown(&fixture());
    // 20pt title → H1, 14pt subheading → H2, "1.1 …" (body size) → H2 via numbering.
    assert!(md.contains("# Document Title"), "expected H1, got:\n{md}");
    assert!(md.contains("## Introduction"), "expected H2, got:\n{md}");
    assert!(md.contains("## 1.1 Methods"), "expected numbered H2, got:\n{md}");
    // Body text must not be promoted to a heading.
    assert!(!md.contains("# This is ordinary body"), "body should not be a heading:\n{md}");
}

#[test]
fn heading_levels_populated_on_paragraphs() {
    let doc = fixture();
    let levels: Vec<(Option<u8>, String)> = doc.pages[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some((p.heading_level, p.text.clone())),
            _ => None,
        })
        .collect();
    let title = levels.iter().find(|(_, t)| t.contains("Document Title")).expect("title block");
    assert_eq!(title.0, Some(1));
    let body = levels.iter().find(|(_, t)| t.starts_with("This is ordinary")).expect("body block");
    assert_eq!(body.0, None, "body paragraph must not be a heading");
}
