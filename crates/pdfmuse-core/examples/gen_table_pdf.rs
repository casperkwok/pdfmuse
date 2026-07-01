//! Generate a minimal one-page PDF with a 2×2 ruled table — a test fixture.
//!
//! Run: `cargo run -p pdfmuse-core --example gen_table_pdf > tests/corpus/table.pdf`
//!
//! Grid lines (user space): x ∈ {100,150,200}, y ∈ {700,720,740}; a single glyph
//! centered in each of the four cells.

use std::io::Write;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

fn line(ops: &mut Vec<Operation>, x0: i32, y0: i32, x1: i32, y1: i32) {
    ops.push(Operation::new("m", vec![x0.into(), y0.into()]));
    ops.push(Operation::new("l", vec![x1.into(), y1.into()]));
}

fn glyph(ops: &mut Vec<Operation>, x: i32, y: i32, text: &str) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new("Tf", vec!["F1".into(), 10.into()]));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    ops.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    ops.push(Operation::new("ET", vec![]));
}

fn main() {
    let mut ops = Vec::new();
    // Verticals then horizontals, all stroked as one path.
    line(&mut ops, 100, 700, 100, 740);
    line(&mut ops, 150, 700, 150, 740);
    line(&mut ops, 200, 700, 200, 740);
    line(&mut ops, 100, 700, 200, 700);
    line(&mut ops, 100, 720, 200, 720);
    line(&mut ops, 100, 740, 200, 740);
    ops.push(Operation::new("S", vec![]));
    // One glyph per cell.
    glyph(&mut ops, 120, 723, "A");
    glyph(&mut ops, 170, 723, "B");
    glyph(&mut ops, 120, 703, "C");
    glyph(&mut ops, 170, 703, "D");

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier" });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
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
    doc.save_to(&mut buf).expect("serialize table PDF");
    std::io::stdout().write_all(&buf).expect("write PDF");
}
