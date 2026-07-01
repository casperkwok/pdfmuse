//! Encrypted-PDF handling (PER-50).
//!
//! `encrypted.pdf` is RC4-encrypted with user password "secret". Parsing must:
//! reject it without a password, decrypt with the right one, and reject a wrong
//! one — never panic, and never leak the password.

use std::path::PathBuf;

use pdfmuse_core::{parse, parse_with_password, PdfmuseError};

fn encrypted() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/encrypted.pdf");
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn no_password_is_rejected() {
    assert!(matches!(parse(&encrypted(), None).unwrap_err(), PdfmuseError::EncryptedNoPassword));
}

#[test]
fn wrong_password_is_rejected() {
    let err = parse_with_password(&encrypted(), None, Some("wrong")).unwrap_err();
    assert!(matches!(err, PdfmuseError::EncryptedNoPassword));
}

#[test]
fn correct_password_decrypts() {
    let doc = parse_with_password(&encrypted(), None, Some("secret")).expect("decrypts");
    let text: String = doc.pages[0].chars.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(text, "Hello pdfmuse");
}
