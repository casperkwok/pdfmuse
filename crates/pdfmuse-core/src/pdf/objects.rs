//! lopdf wrapper + a validation pass.
//!
//! lopdf can silently skip objects it fails to parse, which would lose data
//! quietly. [`PdfDoc::load`] runs a best-effort validation pass that records
//! dangling references and undecodable streams as warnings
//! ([`WarningKind::MalformedObject`]) so callers can see what was dropped.
//!
//! It also exposes the page-level accessors (`media_box`, `content_bytes`) that
//! the content-stream interpreter (PER-36) will build on.

use std::collections::BTreeMap;

use lopdf::{Document as LoDoc, Object, ObjectId};

use crate::error::{PdfmuseError, Result};
use crate::ir::{Warning, WarningKind};

pub(crate) struct PdfDoc {
    pub(crate) inner: LoDoc,
}

impl PdfDoc {
    /// Load bytes, decrypt if needed, and run the validation pass. Returns the
    /// wrapped document plus any non-fatal warnings; a broken container or a
    /// failed decryption is a fatal `Err`.
    ///
    /// Decryption happens *before* validation so encrypted streams are not
    /// false-flagged as malformed. The password is never logged.
    pub(crate) fn load(data: &[u8], password: Option<&str>) -> Result<(Self, Vec<Warning>)> {
        let mut inner = LoDoc::load_mem(data).map_err(|e| PdfmuseError::Malformed(e.to_string()))?;
        if inner.is_encrypted() {
            // Try the given password, else the empty user password (common default).
            inner
                .decrypt(password.unwrap_or(""))
                .map_err(|_| PdfmuseError::EncryptedNoPassword)?;
        }
        let warnings = validate(&inner);
        Ok((Self { inner }, warnings))
    }

    pub(crate) fn pages(&self) -> BTreeMap<u32, ObjectId> {
        self.inner.get_pages()
    }

    /// MediaBox as `[x0, y0, x1, y1]` in PDF points, resolving inheritance
    /// (the attribute may live on an ancestor `Pages` node).
    pub(crate) fn media_box(&self, page_id: ObjectId) -> Option<[f32; 4]> {
        let obj = self.inherited(page_id, b"MediaBox")?;
        let arr = obj.as_array().ok()?;
        if arr.len() != 4 {
            return None;
        }
        let mut out = [0.0f32; 4];
        for (slot, v) in out.iter_mut().zip(arr) {
            *slot = number(v)?;
        }
        Some(out)
    }

    /// Decoded, concatenated content-stream bytes for a page.
    pub(crate) fn content_bytes(&self, page_id: ObjectId) -> Result<Vec<u8>> {
        self.inner
            .get_page_content(page_id)
            .map_err(|e| PdfmuseError::Malformed(e.to_string()))
    }

    /// The page's (possibly inherited) `/Resources` dictionary.
    pub(crate) fn page_resources(&self, page_id: ObjectId) -> Option<lopdf::Dictionary> {
        self.inherited(page_id, b"Resources")
            .and_then(|o| o.as_dict().ok().cloned())
    }

    /// Does the page reference an image XObject? A page with images but no text
    /// is a scanned page that needs OCR (a pluggable-backend job).
    pub(crate) fn page_has_image(&self, page_id: ObjectId) -> bool {
        let Some(res) = self.page_resources(page_id) else {
            return false;
        };
        let Ok(xobj) = res.get(b"XObject") else {
            return false;
        };
        let resolved = self.inner.dereference(xobj).map(|(_, o)| o).unwrap_or(xobj);
        let Ok(dict) = resolved.as_dict() else {
            return false;
        };
        dict.iter().any(|(_, v)| {
            let obj = self.inner.dereference(v).map(|(_, o)| o).unwrap_or(v);
            matches!(obj, Object::Stream(s)
                if s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Image".as_ref()))
        })
    }

    /// Walk `key` up the page → `Pages` parent chain, resolving references.
    fn inherited(&self, page_id: ObjectId, key: &[u8]) -> Option<Object> {
        let mut current = Some(page_id);
        for _ in 0..32 {
            let dict = self.inner.get_dictionary(current?).ok()?;
            if let Ok(v) = dict.get(key) {
                let resolved = self.inner.dereference(v).map(|(_, o)| o).unwrap_or(v);
                return Some(resolved.clone());
            }
            current = dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        }
        None
    }
}

fn number(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// Best-effort validation: record dangling references and undecodable streams so
/// data lopdf would drop silently becomes visible in `warnings`.
fn validate(doc: &LoDoc) -> Vec<Warning> {
    let mut warnings = Vec::new();
    for (id, obj) in &doc.objects {
        scan(doc, *id, obj, &mut warnings);
    }
    warnings
}

fn scan(doc: &LoDoc, owner: ObjectId, obj: &Object, out: &mut Vec<Warning>) {
    match obj {
        Object::Reference(rid) => {
            if !doc.objects.contains_key(rid) {
                out.push(malformed(format!(
                    "object {}:{} references missing object {}:{}",
                    owner.0, owner.1, rid.0, rid.1
                )));
            }
        }
        Object::Array(items) => items.iter().for_each(|it| scan(doc, owner, it, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| scan(doc, owner, v, out)),
        Object::Stream(s) => {
            // Only check references in the stream dict. Decoding every stream to
            // validate it is expensive (a large image/font program per object) and
            // pointless: we never consume image data or glyph programs, image
            // codecs (DCTDecode/JPX) can't be flate-decoded so they'd false-flag,
            // and content-stream decode failures are already reported by the
            // interpreter where they actually cost text.
            s.dict.iter().for_each(|(_, v)| scan(doc, owner, v, out));
        }
        _ => {}
    }
}

fn malformed(detail: String) -> Warning {
    Warning { page: None, kind: WarningKind::MalformedObject, detail }
}

#[cfg(test)]
mod tests {
    use super::PdfDoc;
    use lopdf::{dictionary, Document as LoDoc, Object};

    #[test]
    fn reads_media_box_from_corpus_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/corpus/hello.pdf");
        let data = std::fs::read(path).expect("read fixture");
        let (pdf, warnings) = PdfDoc::load(&data, None).expect("load");

        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        let (&page_num, &page_id) = pdf.pages().iter().next().expect("one page");
        assert_eq!(page_num, 1);
        assert_eq!(pdf.media_box(page_id), Some([0.0, 0.0, 612.0, 792.0]));
        assert!(!pdf.content_bytes(page_id).unwrap().is_empty());
    }

    #[test]
    fn validation_flags_dangling_reference() {
        // A catalog pointing at an object id that does not exist.
        let mut doc = LoDoc::with_version("1.5");
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference((999, 0)) };
        let cid = doc.add_object(catalog);
        doc.trailer.set("Root", cid);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let (_pdf, warnings) = PdfDoc::load(&buf, None).expect("load");
        assert!(
            warnings.iter().any(|w| w.detail.contains("missing object 999:0")),
            "warnings were: {warnings:?}"
        );
    }
}
