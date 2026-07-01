//! Markdown rendering of the IR.
//!
//! Walks pages and their reading-order [`Block`]s, emitting paragraphs (with
//! `#` heading levels) and GitHub-flavored tables. Purely geometric input in,
//! deterministic Markdown out.

use crate::ir::{Block, Document, Paragraph, Table};

/// Render `doc` to GitHub-flavored Markdown, pages and blocks in order.
pub fn to_markdown(doc: &Document) -> String {
    let mut blocks = Vec::new();
    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Paragraph(p) => blocks.push(paragraph_md(p)),
                Block::Table(t) => blocks.push(table_md(t)),
                // Images have no textual Markdown body in the IR; skipped.
                Block::Image(_) => {}
            }
        }
    }
    // One blank line between every block (and thus between pages too).
    blocks.join("\n\n")
}

/// A heading paragraph becomes `#`-prefixed; a normal one is its text verbatim.
fn paragraph_md(p: &Paragraph) -> String {
    match p.heading_level {
        Some(n) if n > 0 => format!("{} {}", "#".repeat(n as usize), p.text),
        _ => p.text.clone(),
    }
}

/// Render a [`Table`] as a GitHub Markdown table (first row = header).
fn table_md(table: &Table) -> String {
    // Expand col-spans so every logical column gets a cell, then pad short rows
    // to the widest row so the grid stays rectangular.
    let expanded: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| {
            let mut cols = Vec::new();
            for cell in row {
                let span = cell.col_span.max(1) as usize;
                let text = escape_cell(&cell.text);
                for _ in 0..span {
                    cols.push(text.clone());
                }
            }
            cols
        })
        .collect();

    let width = expanded.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }

    let mut out = String::new();
    for (i, row) in expanded.iter().enumerate() {
        let mut cells: Vec<String> = row.clone();
        cells.resize(width, String::new());
        out.push_str(&format!("| {} |", cells.join(" | ")));
        out.push('\n');
        if i == 0 {
            // Header separator row.
            let sep = vec!["---"; width].join(" | ");
            out.push_str(&format!("| {sep} |"));
            out.push('\n');
        }
    }
    // Trim the trailing newline so the caller controls block spacing.
    out.pop();
    out
}

/// Escape characters that would break a Markdown table cell.
fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}
