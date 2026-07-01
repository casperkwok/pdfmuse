//! Generate a minimal `.docx` fixture (heading + paragraph + merged-cell table).
//!
//! Run: `cargo run -p pdfmuse-core --example gen_docx > tests/corpus/sample.docx`
//!
//! Only `word/document.xml` + `word/styles.xml` are written — the two parts
//! pdfmuse reads. (Not a fully valid Office package, but sufficient as a fixture.)

use std::io::Write;

use zip::write::{SimpleFileOptions, ZipWriter};

const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Report</w:t></w:r></w:p>
    <w:p><w:r><w:t xml:space="preserve">Intro paragraph.</w:t></w:r></w:p>
    <w:tbl>
      <w:tr>
        <w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:tc>
      </w:tr>
      <w:tr>
        <w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>
        <w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>
</w:styles>"#;

fn main() {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default();
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(DOCUMENT.as_bytes()).unwrap();
        zw.start_file("word/styles.xml", opts).unwrap();
        zw.write_all(STYLES.as_bytes()).unwrap();
        zw.finish().unwrap();
    }
    std::io::stdout().write_all(&buf).expect("write DOCX");
}
