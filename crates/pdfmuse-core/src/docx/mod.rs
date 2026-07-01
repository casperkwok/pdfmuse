//! DOCX (OOXML wordprocessing) parsing.
//!
//! A `.docx` is a ZIP container of XML parts. Unlike PDF, it carries no page
//! geometry or coordinates — it is a logical stream of paragraphs and tables.
//! We therefore lower it to a single synthetic [`Page`] (index 0, size 0×0, no
//! chars) whose [`blocks`](Page::blocks) hold all content in document order.
//!
//! Only two parts are consulted:
//! - `word/document.xml` — the body (paragraphs + tables); **required**.
//! - `word/styles.xml` — style-id → heading-level table; **optional**.
//!
//! Fatal problems (not a ZIP, missing `document.xml`, unreadable XML) surface as
//! [`PdfmuseError::Malformed`]; everything else degrades gracefully.

mod document;
mod styles;

use std::collections::HashMap;
use std::io::{BufRead, Cursor, Read, Seek};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::error::{PdfmuseError, Result};
use crate::ir::{Document, Metadata, Page, SourceKind};

/// Parse the bytes of a `.docx` file into the unified IR.
pub(crate) fn parse(data: &[u8]) -> Result<Document> {
    let mut zip = ZipArchive::new(Cursor::new(data))
        .map_err(|e| PdfmuseError::Malformed(format!("not a valid DOCX (ZIP) container: {e}")))?;

    // word/document.xml is mandatory — its absence means this is not a DOCX body.
    let document_xml = read_entry(&mut zip, "word/document.xml")?
        .ok_or_else(|| PdfmuseError::Malformed("DOCX is missing word/document.xml".to_string()))?;

    // word/styles.xml is optional; without it, headings resolve from the style-id
    // form (`Heading{N}`) alone.
    let styles = match read_entry(&mut zip, "word/styles.xml")? {
        Some(bytes) => styles::parse_styles(&bytes)?,
        None => HashMap::new(),
    };

    let blocks = document::parse_document(&document_xml, &styles)?;

    // DOCX has no pages/coordinates: one synthetic page carries every block.
    let page = Page { index: 0, width: 0.0, height: 0.0, blocks, ..Default::default() };
    Ok(Document {
        source: SourceKind::Docx,
        metadata: Metadata { page_count: 1, ..Default::default() },
        pages: vec![page],
        ..Default::default()
    })
}

/// Read a ZIP entry fully. Returns `Ok(None)` when the entry is simply absent
/// (so optional parts don't error), and `Err` only on a genuine read failure.
fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<Option<Vec<u8>>> {
    match zip.by_name(name) {
        Ok(mut file) => {
            let mut buf = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buf)?;
            Ok(Some(buf))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(PdfmuseError::Malformed(format!("cannot read {name} from DOCX: {e}"))),
    }
}

/// Look up an attribute by its local name (namespace prefix ignored) and return
/// its value as an owned `String`. Values here are ASCII (style ids, spans),
/// so a lossy UTF-8 decode is safe and deterministic.
fn attr_by_local(e: &BytesStart, local: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        (a.key.local_name().as_ref() == local)
            .then(|| String::from_utf8_lossy(&a.value).into_owned())
    })
}

/// Read the next XML event, mapping any parse failure to a fatal [`Malformed`].
///
/// [`Malformed`]: PdfmuseError::Malformed
fn next_event<'b, R: BufRead>(reader: &mut Reader<R>, buf: &'b mut Vec<u8>) -> Result<Event<'b>> {
    reader
        .read_event_into(buf)
        .map_err(|e| PdfmuseError::Malformed(format!("invalid DOCX XML: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Block, TableSource};
    use std::io::Write;
    use zip::write::{SimpleFileOptions, ZipWriter};

    /// Build a minimal `.docx` in memory from raw XML parts.
    fn build_docx(document_xml: &str, styles_xml: Option<&str>) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            zw.start_file("word/document.xml", opts).unwrap();
            zw.write_all(document_xml.as_bytes()).unwrap();
            if let Some(s) = styles_xml {
                zw.start_file("word/styles.xml", opts).unwrap();
                zw.write_all(s.as_bytes()).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    /// A Heading-1 title, a normal paragraph, and a 2×2 table whose first row is
    /// a single `gridSpan=2` merged cell.
    const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>
    <w:p><w:r><w:t xml:space="preserve">Body text</w:t></w:r></w:p>
    <w:tbl>
      <w:tr>
        <w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged</w:t></w:r></w:p></w:tc>
      </w:tr>
      <w:tr>
        <w:tc><w:p><w:r><w:t>R2C1</w:t></w:r></w:p></w:tc>
        <w:tc><w:p><w:r><w:t>R2C2</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

    const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>
</w:styles>"#;

    #[test]
    fn parses_headings_paragraphs_and_merged_table() {
        let doc = parse(&build_docx(DOC, Some(STYLES))).expect("parses a DOCX");
        assert_eq!(doc.source, SourceKind::Docx);
        assert_eq!(doc.pages.len(), 1);

        let blocks = &doc.pages[0].blocks;
        assert_eq!(blocks.len(), 3);

        match &blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.text, "Title");
                assert_eq!(p.heading_level, Some(1));
            }
            other => panic!("expected heading paragraph, got {other:?}"),
        }
        match &blocks[1] {
            Block::Paragraph(p) => {
                assert_eq!(p.text, "Body text");
                assert_eq!(p.heading_level, None);
            }
            other => panic!("expected body paragraph, got {other:?}"),
        }
        match &blocks[2] {
            Block::Table(t) => {
                assert_eq!(t.source, TableSource::Docx);
                assert_eq!(t.rows.len(), 2);
                // Row 0: a single cell spanning both columns.
                assert_eq!(t.rows[0].len(), 1);
                assert_eq!(t.rows[0][0].text, "Merged");
                assert_eq!(t.rows[0][0].col_span, 2);
                assert_eq!(t.rows[0][0].row_span, 1);
                // Row 1: two ordinary cells.
                assert_eq!(t.rows[1].len(), 2);
                assert_eq!(t.rows[1][0].text, "R2C1");
                assert_eq!(t.rows[1][1].text, "R2C2");
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn heading_level_resolves_without_styles_xml() {
        // No styles.xml: the `Heading1` style id must still resolve via the fallback.
        let doc = parse(&build_docx(DOC, None)).unwrap();
        match &doc.pages[0].blocks[0] {
            Block::Paragraph(p) => assert_eq!(p.heading_level, Some(1)),
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn vertical_merge_sets_row_span_and_omits_covered_cells() {
        let doc_xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
  <w:tbl>
    <w:tr>
      <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>Span</w:t></w:r></w:p></w:tc>
      <w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
    </w:tr>
    <w:tr>
      <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p><w:r><w:t>hidden</w:t></w:r></w:p></w:tc>
      <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
    </w:tr>
  </w:tbl>
</w:body></w:document>"#;
        let doc = parse(&build_docx(doc_xml, None)).unwrap();
        match &doc.pages[0].blocks[0] {
            Block::Table(t) => {
                assert_eq!(t.rows[0].len(), 2);
                assert_eq!(t.rows[0][0].text, "Span");
                assert_eq!(t.rows[0][0].row_span, 2);
                // The covered cell in row 1 is omitted; only "B" survives.
                assert_eq!(t.rows[1].len(), 1);
                assert_eq!(t.rows[1][0].text, "B");
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn non_zip_bytes_are_malformed() {
        assert!(matches!(parse(b"definitely not a zip"), Err(PdfmuseError::Malformed(_))));
    }

    #[test]
    fn zip_without_document_xml_is_malformed() {
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(Cursor::new(&mut buf));
            zw.start_file("word/other.xml", SimpleFileOptions::default()).unwrap();
            zw.write_all(b"<x/>").unwrap();
            zw.finish().unwrap();
        }
        assert!(matches!(parse(&buf), Err(PdfmuseError::Malformed(_))));
    }
}
