//! PDF parsing.
//!
//! M0 is a **naive path**: lopdf loads the object tree ([`objects`]) and
//! `extract_text` pulls plain text (one paragraph per page, no coordinates). The
//! real value — a self-written content-stream interpreter that emits chars with
//! precise bboxes — replaces the text step in PER-36, building on the page
//! accessors in [`objects`].

mod fonts;
mod objects;
mod tables;

use crate::error::{PdfmuseError, Result};
use crate::ir::{BBox, Block, Document, Metadata, Page, Paragraph, SourceKind, Warning, WarningKind};
use objects::PdfDoc;

/// Naive PDF → IR. Fills one [`Paragraph`] per page from `extract_text` and page
/// dimensions from the MediaBox; leaves `chars`/`lines` empty until the
/// content-stream interpreter (PER-36) lands.
pub(crate) fn parse_pdf(data: &[u8]) -> Result<Document> {
    let (pdf, warnings) = PdfDoc::load(data)?;

    // Encrypted documents need a password — support arrives in PER-50. The
    // password (once supported) is never logged.
    if pdf.is_encrypted() {
        return Err(PdfmuseError::EncryptedNoPassword);
    }

    let pages = pdf.pages();
    let mut out = Document {
        source: SourceKind::Pdf,
        metadata: Metadata { page_count: pages.len() as u32, ..Default::default() },
        warnings, // dangling refs / undecodable streams surfaced by the validation pass
        ..Default::default()
    };

    for (page_number, page_id) in pages {
        // lopdf page numbers are 1-based; the IR is 0-based.
        let index = page_number.saturating_sub(1);
        let mut page = Page { index, ..Default::default() };

        // Page dimensions from the (possibly inherited) MediaBox.
        if let Some([x0, y0, x1, y1]) = pdf.media_box(page_id) {
            page.width = (x1 - x0).abs();
            page.height = (y1 - y0).abs();
        }

        match pdf.inner.extract_text(&[page_number]) {
            Ok(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    page.blocks.push(Block::Paragraph(Paragraph {
                        bbox: BBox::default(),
                        text: text.to_string(),
                        heading_level: None,
                    }));
                }
            }
            // A single unreadable page degrades gracefully: record and continue,
            // never abort the whole document.
            Err(e) => out.warnings.push(Warning {
                page: Some(index),
                kind: WarningKind::MalformedObject,
                detail: format!("text extraction failed: {e}"),
            }),
        }

        out.pages.push(page);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse_pdf;
    use crate::ir::{Block, SourceKind};
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document as LoDoc, Object, Stream};

    /// Synthesize a minimal one-page digital PDF containing `text`.
    fn sample_pdf(text: &str) -> Vec<u8> {
        let mut doc = LoDoc::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 24.into()]),
                Operation::new("Td", vec![100.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
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
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[test]
    fn extracts_non_empty_text_from_digital_pdf() {
        let bytes = sample_pdf("Hello pdfmuse");
        let doc = parse_pdf(&bytes).expect("parses a digital PDF");
        assert_eq!(doc.source, SourceKind::Pdf);
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.metadata.page_count, 1);
        assert!(doc.warnings.is_empty(), "unexpected warnings: {:?}", doc.warnings);
        assert_eq!((doc.pages[0].width, doc.pages[0].height), (612.0, 792.0));

        let text: String = doc.pages[0]
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => Some(p.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains("Hello pdfmuse"), "extracted text was: {text:?}");
    }
}
