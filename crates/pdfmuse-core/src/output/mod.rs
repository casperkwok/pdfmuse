//! Output layer — serialization of the unified [`ir`](crate::ir) into
//! consumer-facing formats.
//!
//! Three deterministic renderings share the one IR: full-fidelity JSON
//! ([`to_json`]), human-readable Markdown ([`to_markdown`]), and RAG-ready
//! [`Chunk`]s ([`chunk`]). Each is a pure function of the [`Document`](crate::ir::Document),
//! so identical input yields byte-identical output across every binding.

mod chunk;
mod json;
mod markdown;

pub use chunk::{chunk, Chunk};
pub use json::to_json;
pub use markdown::to_markdown;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        BBox, Block, Cell, Document, Page, Paragraph, Table, TableSource,
    };

    fn bbox() -> BBox {
        BBox { x0: 0.0, y0: 0.0, x1: 10.0, y1: 10.0 }
    }

    /// A page with a heading, a paragraph, and a 2x2 ruled table.
    fn sample_doc() -> Document {
        let cell = |t: &str| Cell {
            text: t.into(),
            bbox: bbox(),
            row_span: 1,
            col_span: 1,
        };
        Document {
            pages: vec![Page {
                index: 0,
                blocks: vec![
                    Block::Paragraph(Paragraph {
                        bbox: bbox(),
                        text: "Title".into(),
                        heading_level: Some(1),
                    }),
                    Block::Paragraph(Paragraph {
                        bbox: bbox(),
                        text: "Hello world".into(),
                        heading_level: None,
                    }),
                    Block::Table(Table {
                        bbox: bbox(),
                        rows: vec![
                            vec![cell("a"), cell("b")],
                            vec![cell("c"), cell("d")],
                        ],
                        source: TableSource::Ruled,
                    }),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn markdown_renders_heading_paragraph_and_table() {
        let md = to_markdown(&sample_doc());
        assert!(md.starts_with("# Title"), "got: {md}");
        assert!(md.contains("Hello world"));
        // Header separator row and cell text present.
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| a | b |"));
        assert!(md.contains("| c | d |"));
    }

    #[test]
    fn json_round_trips() {
        let json = to_json(&sample_doc());
        assert!(!json.is_empty());
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("JSON parses back");
        assert!(value.get("pages").is_some());
    }

    #[test]
    fn chunks_carry_heading_path_page_and_keep_table_whole() {
        let chunks = chunk(&sample_doc());
        // Title, Hello world, table → 3 chunks (table is one, not four).
        assert_eq!(chunks.len(), 3);
        // The heading sets the path before it is emitted, so its own chunk
        // already carries itself.
        assert_eq!(chunks[0].heading_path, vec!["Title".to_string()]);
        assert_eq!(chunks[0].text, "Title");
        assert_eq!(chunks[1].text, "Hello world");
        assert_eq!(chunks[1].heading_path, vec!["Title".to_string()]);
        // Table chunk carries the heading path too and stays a single unit.
        assert_eq!(chunks[2].heading_path, vec!["Title".to_string()]);
        assert!(chunks[2].text.contains("a | b"));
        assert!(chunks[2].text.contains("c | d"));
        for c in &chunks {
            assert_eq!(c.page, 0);
        }
    }
}
