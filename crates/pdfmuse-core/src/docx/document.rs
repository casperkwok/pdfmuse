//! Parse `word/document.xml` into ordered IR [`Block`]s.
//!
//! The body is a flat stream of `w:p` (paragraphs) and `w:tbl` (tables). We walk
//! it with a streaming [`Reader`]: on each top-level `w:p`/`w:tbl` we consume that
//! element's entire subtree, so paragraphs nested inside table cells are handled
//! by the table reader and never leak out as top-level blocks. Document order is
//! preserved by construction.

use std::collections::HashMap;
use std::io::BufRead;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::error::Result;
use crate::ir::{BBox, Block, Cell, Paragraph, Table, TableSource};

use super::{attr_by_local, next_event, styles};

/// Parse the document body into blocks in reading order.
pub(super) fn parse_document(xml: &[u8], styles: &HashMap<String, u8>) -> Result<Vec<Block>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut blocks = Vec::new();
    loop {
        match next_event(&mut reader, &mut buf)? {
            Event::Start(e) => match e.local_name().as_ref() {
                b"p" => blocks.push(Block::Paragraph(read_paragraph(&mut reader, styles)?)),
                b"tbl" => blocks.push(Block::Table(read_table(&mut reader)?)),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(blocks)
}

/// Consume a `w:p` subtree (start already read) into a [`Paragraph`].
///
/// Text is the concatenation of every descendant `w:t`; the heading level comes
/// from `w:pPr/w:pStyle @w:val` resolved through the styles table.
fn read_paragraph<R: BufRead>(
    reader: &mut Reader<R>,
    styles: &HashMap<String, u8>,
) -> Result<Paragraph> {
    let mut buf = Vec::new();
    let mut text = String::new();
    let mut style_id: Option<String> = None;
    let mut in_text = false;
    loop {
        match next_event(reader, &mut buf)? {
            Event::Start(e) => match e.local_name().as_ref() {
                b"t" => in_text = true,
                b"pStyle" => style_id = attr_by_local(&e, b"val"),
                _ => {}
            },
            // pStyle is usually the self-closing form `<w:pStyle w:val=".."/>`.
            Event::Empty(e) if e.local_name().as_ref() == b"pStyle" => {
                style_id = attr_by_local(&e, b"val");
            }
            // Only `w:t` runs contribute text; `xml:space="preserve"` is honored by
            // reading the literal content verbatim.
            Event::Text(e) if in_text => {
                let t = e
                    .unescape()
                    .map_err(|err| crate::error::PdfmuseError::Malformed(format!("invalid DOCX text: {err}")))?;
                text.push_str(&t);
            }
            Event::End(e) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"p" => break,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    let heading_level = style_id.as_deref().and_then(|id| styles::heading_level(styles, id));
    Ok(Paragraph { bbox: BBox::default(), text, heading_level })
}

/// A cell as read from XML, before vertical-merge resolution. `grid_col` is the
/// starting grid column, filled in once the whole table is known.
struct RawCell {
    text: String,
    col_span: u16,
    vmerge: VMerge,
    grid_col: u32,
}

/// Vertical-merge (`w:vMerge`) state of a cell.
enum VMerge {
    /// No vertical merge — an ordinary cell.
    None,
    /// `val="restart"` — the top of a vertical span.
    Restart,
    /// `val="continue"` or a bare `<w:vMerge/>` — covered by the cell above.
    Continue,
}

/// Consume a `w:tbl` subtree (start already read) into a [`Table`].
fn read_table<R: BufRead>(reader: &mut Reader<R>) -> Result<Table> {
    let mut buf = Vec::new();
    let mut raw_rows: Vec<Vec<RawCell>> = Vec::new();
    loop {
        match next_event(reader, &mut buf)? {
            Event::Start(e) if e.local_name().as_ref() == b"tr" => {
                raw_rows.push(read_row(reader)?);
            }
            Event::End(e) if e.local_name().as_ref() == b"tbl" => break,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(build_table(raw_rows))
}

/// Consume a `w:tr` subtree (start already read) into its cells.
fn read_row<R: BufRead>(reader: &mut Reader<R>) -> Result<Vec<RawCell>> {
    let mut buf = Vec::new();
    let mut cells = Vec::new();
    loop {
        match next_event(reader, &mut buf)? {
            Event::Start(e) if e.local_name().as_ref() == b"tc" => cells.push(read_cell(reader)?),
            Event::End(e) if e.local_name().as_ref() == b"tr" => break,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(cells)
}

/// Consume a `w:tc` subtree (start already read) into a [`RawCell`].
///
/// `tc_depth` guards against nested tables: `gridSpan`/`vMerge`/paragraph
/// separators apply to this cell only (depth 1), while nested-table text is still
/// folded into the cell text (a benign degradation).
fn read_cell<R: BufRead>(reader: &mut Reader<R>) -> Result<RawCell> {
    let mut buf = Vec::new();
    let mut text = String::new();
    let mut col_span = 1u16;
    let mut vmerge = VMerge::None;
    let mut in_text = false;
    let mut tc_depth = 1usize;
    let mut paragraphs = 0usize;
    loop {
        match next_event(reader, &mut buf)? {
            Event::Start(e) => match e.local_name().as_ref() {
                b"tc" => tc_depth += 1, // entering a nested table's cell
                b"t" => in_text = true,
                // Stacked paragraphs in one cell are joined with a newline.
                b"p" if tc_depth == 1 => {
                    if paragraphs > 0 {
                        text.push('\n');
                    }
                    paragraphs += 1;
                }
                b"gridSpan" if tc_depth == 1 => col_span = parse_span(&e),
                b"vMerge" if tc_depth == 1 => vmerge = parse_vmerge(&e),
                _ => {}
            },
            Event::Empty(e) => match e.local_name().as_ref() {
                b"gridSpan" if tc_depth == 1 => col_span = parse_span(&e),
                b"vMerge" if tc_depth == 1 => vmerge = parse_vmerge(&e),
                _ => {}
            },
            Event::Text(e) if in_text => {
                let t = e
                    .unescape()
                    .map_err(|err| crate::error::PdfmuseError::Malformed(format!("invalid DOCX text: {err}")))?;
                text.push_str(&t);
            }
            Event::End(e) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"tc" => {
                    tc_depth -= 1;
                    if tc_depth == 0 {
                        break;
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(RawCell { text, col_span, vmerge, grid_col: 0 })
}

/// `w:gridSpan @w:val` → column span (defaults to 1, never 0).
fn parse_span(e: &BytesStart) -> u16 {
    attr_by_local(e, b"val")
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1)
}

/// `w:vMerge @w:val` → merge state. A present `w:vMerge` without `restart`
/// (including a bare `<w:vMerge/>`) continues the span above.
fn parse_vmerge(e: &BytesStart) -> VMerge {
    match attr_by_local(e, b"val") {
        Some(v) if v.eq_ignore_ascii_case("restart") => VMerge::Restart,
        _ => VMerge::Continue,
    }
}

/// Resolve merges into a row-major grid: assign grid columns, drop `continue`
/// cells (HTML-style), and give each `restart` cell the row span it covers.
fn build_table(mut raw_rows: Vec<Vec<RawCell>>) -> Table {
    // Grid column = running sum of preceding column spans in the row.
    for row in &mut raw_rows {
        let mut col = 0u32;
        for cell in row.iter_mut() {
            cell.grid_col = col;
            col += cell.col_span as u32;
        }
    }

    let mut rows: Vec<Vec<Cell>> = Vec::with_capacity(raw_rows.len());
    for (r, row) in raw_rows.iter().enumerate() {
        let mut out_row = Vec::new();
        for cell in row {
            let row_span = match cell.vmerge {
                VMerge::Continue => continue, // covered by the restart cell above — omit it
                VMerge::Restart => vertical_span(&raw_rows, r, cell.grid_col),
                VMerge::None => 1,
            };
            out_row.push(Cell {
                text: cell.text.clone(),
                bbox: BBox::default(),
                row_span,
                col_span: cell.col_span,
            });
        }
        rows.push(out_row);
    }
    Table { bbox: BBox::default(), rows, source: TableSource::Docx }
}

/// Count how many rows a `restart` cell at `(start, grid_col)` covers: itself plus
/// each subsequent row that has a `continue` cell at the same grid column.
fn vertical_span(rows: &[Vec<RawCell>], start: usize, grid_col: u32) -> u16 {
    let mut span = 1u16;
    for row in rows.iter().skip(start + 1) {
        let continues = row
            .iter()
            .any(|c| c.grid_col == grid_col && matches!(c.vmerge, VMerge::Continue));
        if continues {
            span += 1;
        } else {
            break;
        }
    }
    span
}
