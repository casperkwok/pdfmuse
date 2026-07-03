//! Regression for PER-168: running headers/footers are marked, kept by default,
//! and removed only on opt-in — without touching body text.

use pdfmuse_core::ir::{Block, BlockRole, Document};

fn fixture() -> Document {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/boilerplate.pdf");
    let data = std::fs::read(path).expect("fixture");
    pdfmuse_core::parse(&data, None).expect("parse")
}

#[test]
fn running_header_footer_is_marked_body_is_not() {
    let doc = fixture();
    let mut headers = 0;
    for page in &doc.pages {
        for b in &page.blocks {
            if let Block::Paragraph(p) = b {
                let boiler = p.role == Some(BlockRole::HeaderFooter);
                if p.text.contains("Confidential Report") || p.text.starts_with("Page ") {
                    assert!(boiler, "running element not marked: {:?}", p.text);
                    headers += 1;
                } else {
                    assert!(!boiler, "body wrongly marked: {:?}", p.text);
                }
            }
        }
    }
    assert_eq!(headers, 6, "3 headers + 3 footers expected");
}

#[test]
fn remove_boilerplate_strips_only_boilerplate() {
    let mut doc = fixture();
    // Default output keeps everything.
    assert!(pdfmuse_core::to_text(&doc).contains("Confidential Report"));

    pdfmuse_core::remove_boilerplate(&mut doc);
    let text = pdfmuse_core::to_text(&doc);
    assert!(!text.contains("Confidential Report"), "header should be stripped");
    assert!(!text.contains("Page 1"), "footer should be stripped");
    // Body survives.
    assert!(text.contains("Introduction and background"), "body must survive");
    assert!(text.contains("Results and discussion"), "body must survive");
}

#[test]
fn body_paragraphs_omit_the_role_field_in_json() {
    let json = pdfmuse_core::to_json(&fixture());
    // `role` is serialized only when set, so ordinary output is unchanged.
    assert!(json.contains("HeaderFooter"), "marked roles should serialize");
    let body_idx = json.find("Introduction and background").expect("body present");
    let around = &json[body_idx.saturating_sub(120)..body_idx];
    assert!(!around.contains("role"), "body paragraph must not carry a role key");
}
