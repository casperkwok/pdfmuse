//! Regression fixture (PER-168): a 3-page PDF with a running header ("Confidential
//! Report") and a per-page footer ("Page N") — the classic boilerplate that
//! pollutes RAG chunks. Each page has unique body text.
//!
//! Run: `cargo run -p pdfmuse-core --example gen_boilerplate_pdf > tests/corpus/boilerplate.pdf`

use std::io::Write;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

fn tj(x: i32, y: i32, s: &str) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 12.into()]),
        Operation::new("Td", vec![x.into(), y.into()]),
        Operation::new("Tj", vec![Object::string_literal(s)]),
        Operation::new("ET", vec![]),
    ]
}

fn main() {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });

    let bodies = ["Introduction and background material.", "Methods and experimental setup.", "Results and discussion."];
    let mut kids = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        let mut ops = Vec::new();
        // MediaBox is 792 tall; PDF origin is bottom-left, so y≈770 is the top edge.
        ops.extend(tj(72, 770, "Confidential Report")); // running header (top)
        ops.extend(tj(72, 400, body)); // unique body (middle)
        ops.extend(tj(72, 30, &format!("Page {}", i + 1))); // footer (bottom)
        let content_id = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        kids.push(page_id.into());
    }
    let count = kids.len() as i64;
    let pages = dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    std::io::stdout().write_all(&buf).unwrap();
}
