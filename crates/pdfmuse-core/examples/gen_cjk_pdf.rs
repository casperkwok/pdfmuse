//! Generate a one-page PDF using a Type0/CID font showing Chinese "中文" — a
//! CJK-correctness fixture.
//!
//! Run: `cargo run -p pdfmuse-core --example gen_cjk_pdf > tests/corpus/cjk.pdf`
//!
//! The font is Type0 with Identity-H (2-byte codes) and a ToUnicode CMap mapping
//! code 0x0001 → 中 (U+4E2D) and 0x0002 → 文 (U+6587). The content stream shows
//! the two 2-byte codes.

use std::io::Write;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

fn main() {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let to_unicode = doc.add_object(Stream::new(
        Dictionary::new(),
        b"beginbfchar\n<0001> <4E2D>\n<0002> <6587>\nendbfchar".to_vec(),
    ));
    let cidfont = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "CJKFont",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
        "DW" => 1000,
        "W" => vec![Object::Integer(1), Object::Array(vec![Object::Integer(1000), Object::Integer(1000)])],
    });
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "CJKFont",
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(cidfont)],
        "ToUnicode" => to_unicode,
    });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![100.into(), 700.into()]),
            // Two 2-byte codes: 0x0001 (中), 0x0002 (文).
            Operation::new("Tj", vec![Object::String(vec![0x00, 0x01, 0x00, 0x02], StringFormat::Hexadecimal)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
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
    doc.save_to(&mut buf).expect("serialize CJK PDF");
    std::io::stdout().write_all(&buf).expect("write PDF");
}
