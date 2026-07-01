//! Generate an RC4-encrypted one-page PDF (user password "secret") — a fixture
//! for the encryption path.
//!
//! Run: `cargo run -p pdfmuse-core --example gen_encrypted_pdf > tests/corpus/encrypted.pdf`

use std::io::Write;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream, StringFormat};

fn main() {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier" });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new("Tj", vec![Object::string_literal("Hello pdfmuse")]),
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
    // Encryption key derivation needs a file /ID in the trailer.
    let id = Object::String(b"pdfmusefixtureid".to_vec(), StringFormat::Literal);
    doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));

    // Encrypt with RC4-128; user password "secret", owner password "owner".
    let version = EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: "secret",
        key_length: 128,
        permissions: Permissions::all(),
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt document");

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialize encrypted PDF");
    std::io::stdout().write_all(&buf).expect("write PDF");
}
