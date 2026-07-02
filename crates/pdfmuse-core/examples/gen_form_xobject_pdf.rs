//! Generate a one-page PDF whose text lives inside a **form XObject** invoked by
//! `Do` — the structure Canva/PDFium/design tools emit. The page content stream
//! draws no text directly; a naive extractor that ignores form XObjects gets
//! nothing. Regression fixture for PER-162.
//!
//! Run: `cargo run -p pdfmuse-core --example gen_form_xobject_pdf > tests/corpus/form_xobject.pdf`

use std::io::Write;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

fn main() {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });

    // The form XObject: this is where the text is actually drawn.
    let form_content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![0.into(), 110.into()]),
            Operation::new("Tj", vec![Object::string_literal("Form XObject text")]),
            Operation::new("ET", vec![]),
        ],
    };
    let form_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 400.into(), 150.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        },
        form_content.encode().unwrap(),
    ));

    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! { "Fm1" => form_id },
    });

    // Page content: position, then invoke the form. No text is drawn directly.
    let page_content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", vec![1.into(), 0.into(), 0.into(), 1.into(), 72.into(), 600.into()]),
            Operation::new("Do", vec!["Fm1".into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, page_content.encode().unwrap()));
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
    doc.save_to(&mut buf).expect("serialize form-xobject PDF");
    std::io::stdout().write_all(&buf).expect("write PDF");
}
