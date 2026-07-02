//! Regression for PER-162: text drawn inside a form XObject (invoked via `Do`) —
//! the Canva/PDFium structure — must be extracted, not silently dropped.

#[test]
fn extracts_text_from_form_xobject() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/form_xobject.pdf");
    let data = std::fs::read(path).expect("fixture");
    let doc = pdfmuse_core::parse(&data, None).expect("parse");
    let text: String = doc
        .pages
        .iter()
        .flat_map(|p| p.chars.iter())
        .map(|c| c.text.as_str())
        .collect();
    assert!(
        text.contains("Form XObject text"),
        "form-XObject text should be extracted, got {text:?}"
    );
}
