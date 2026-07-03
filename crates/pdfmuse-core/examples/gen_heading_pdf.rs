//! Regression fixture generator (PER-167): a PDF with a large-font title, a
//! smaller-font subheading, an ASCII multi-level numbered heading, and 10pt body
//! text — exercises both the size-clustering and numbering heading passes.
//!
//! Run: `cargo run -p pdfmuse-core --example gen_heading_pdf > tests/corpus/headings.pdf`

use std::io::Write;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

fn tj(x: i32, y: i32, size: i32, s: &str) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), size.into()]),
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

    let mut ops = Vec::new();
    ops.extend(tj(72, 750, 20, "Document Title")); // 20pt → H1 (size)
    ops.extend(tj(72, 710, 14, "Introduction")); // 14pt → H2 (size)
    for i in 0..3 {
        ops.extend(tj(72, 688 - i * 14, 10, "This is ordinary body text at ten points in size."));
    }
    ops.extend(tj(72, 630, 10, "1.1 Methods")); // numbered, body size → H2 (numbering)
    for i in 0..3 {
        ops.extend(tj(72, 608 - i * 14, 10, "More body text describing the methods used here."));
    }

    let content_id = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    let pages = dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    std::io::stdout().write_all(&buf).unwrap();
}
