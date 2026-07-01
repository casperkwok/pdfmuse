//! RAG chunking of the IR.
//!
//! Emits one [`Chunk`] per block, each carrying the block's page, bbox, and the
//! running heading path (the stack of enclosing headings). Tables are never
//! split — a table becomes exactly one chunk with a flattened text body.

use crate::ir::{BBox, Block, Document, Table};
use serde::Serialize;

/// A retrieval unit: a block's text plus the context needed to cite it.
#[derive(Serialize, Clone, Debug)]
pub struct Chunk {
    pub text: String,
    pub page: u32,
    pub bbox: BBox,
    /// The stack of enclosing headings, outermost first.
    pub heading_path: Vec<String>,
}

/// Split `doc` into chunks (one per non-empty block), tracking heading context.
pub fn chunk(doc: &Document) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    // `heading_path[i]` is the current heading at level `i + 1`.
    let mut heading_path: Vec<String> = Vec::new();

    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Paragraph(p) => {
                    if let Some(level) = p.heading_level.filter(|&n| n > 0) {
                        // A heading sets the path at its depth and drops deeper
                        // levels; missing intermediate levels are padded blank.
                        let depth = level as usize;
                        heading_path.truncate(depth.saturating_sub(1));
                        heading_path.resize(depth - 1, String::new());
                        heading_path.push(p.text.clone());
                    }
                    if p.text.trim().is_empty() {
                        continue;
                    }
                    chunks.push(Chunk {
                        text: p.text.clone(),
                        page: page.index,
                        bbox: p.bbox,
                        heading_path: heading_path.clone(),
                    });
                }
                Block::Table(t) => {
                    let text = flatten_table(t);
                    if text.trim().is_empty() {
                        continue;
                    }
                    chunks.push(Chunk {
                        text,
                        page: page.index,
                        bbox: t.bbox,
                        heading_path: heading_path.clone(),
                    });
                }
                // Images carry no text to embed.
                Block::Image(_) => {}
            }
        }
    }
    chunks
}

/// Flatten a table into a single readable string: cells joined by " | " per row,
/// rows joined by newlines.
fn flatten_table(table: &Table) -> String {
    table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Cell, Paragraph, Table, TableSource};

    fn bbox() -> BBox {
        BBox { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0 }
    }

    #[test]
    fn heading_path_tracks_nesting() {
        let doc = Document {
            pages: vec![crate::ir::Page {
                index: 3,
                blocks: vec![
                    Block::Paragraph(Paragraph {
                        bbox: bbox(),
                        text: "Title".into(),
                        heading_level: Some(1),
                    }),
                    Block::Paragraph(Paragraph {
                        bbox: bbox(),
                        text: "Body".into(),
                        heading_level: None,
                    }),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let chunks = chunk(&doc);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1].heading_path, vec!["Title".to_string()]);
        assert_eq!(chunks[1].page, 3);
    }

    #[test]
    fn table_is_one_chunk() {
        let cell = |t: &str| Cell {
            text: t.into(),
            bbox: bbox(),
            row_span: 1,
            col_span: 1,
        };
        let doc = Document {
            pages: vec![crate::ir::Page {
                index: 0,
                blocks: vec![Block::Table(Table {
                    bbox: bbox(),
                    rows: vec![vec![cell("a"), cell("b")], vec![cell("c"), cell("d")]],
                    source: TableSource::Ruled,
                })],
                ..Default::default()
            }],
            ..Default::default()
        };
        let chunks = chunk(&doc);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("a | b"));
    }
}
