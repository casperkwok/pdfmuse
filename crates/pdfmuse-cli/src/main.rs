//! pdfmuse-cli — a debug command line for inspecting the parsed IR.
//!
//! It is deliberately dependency-light (manual arg parsing, no `clap`): the CLI
//! is a developer aid, not a product surface. It reads a file, hands the raw
//! bytes to [`pdfmuse_core::parse`] (letting the core auto-detect the format),
//! and dumps the resulting [`pdfmuse_core::ir::Document`] as either pretty JSON
//! or a plain-text Markdown stand-in.
//!
//! Usage: `pdfmuse parse <FILE> [--format json|md] [--debug]`
//!
//! `--format` selects the *output* representation (input format is detected by
//! the core). `--debug` writes per-page diagnostics to stderr. Any failure —
//! bad arguments, an unreadable file, or a parse error — prints a clear message
//! to stderr and exits with code 1.

use std::fmt::Write as _;
use std::process::ExitCode;

use pdfmuse_core::ir::Document;

/// The output representation chosen via `--format`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    /// Pretty-printed serde JSON of the whole IR (default).
    Json,
    /// A plain-text dump: each page's chars concatenated, pages separated by a
    /// horizontal rule. Real Markdown structure arrives in a later issue.
    Markdown,
}

/// Parsed command-line arguments for the `parse` subcommand.
struct Args {
    file: String,
    format: OutputFormat,
    debug: bool,
}

fn main() -> ExitCode {
    // Skip argv[0] (the binary path); everything after is user input.
    match run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Drive the whole flow, funnelling every failure into a single `Err(String)`
/// so `main` can render it uniformly and exit non-zero.
fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;

    let bytes = std::fs::read(&args.file)
        .map_err(|e| format!("cannot read '{}': {e}", args.file))?;

    let doc = pdfmuse_core::parse(&bytes, None).map_err(|e| e.to_string())?;

    if args.debug {
        print_debug(&doc);
    }

    match args.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&doc)
                .map_err(|e| format!("failed to serialize document to JSON: {e}"))?;
            println!("{json}");
        }
        OutputFormat::Markdown => print!("{}", render_markdown(&doc)),
    }

    Ok(())
}

/// Parse the `parse <FILE> [--format json|md] [--debug]` invocation by hand.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = args;

    match args.next().as_deref() {
        Some("parse") => {}
        Some(other) => return Err(format!("unknown command '{other}'\n{USAGE}")),
        None => return Err(format!("missing command\n{USAGE}")),
    }

    let mut file: Option<String> = None;
    let mut format = OutputFormat::Json;
    let mut debug = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("--format needs a value (json|md)\n{USAGE}"))?;
                format = match value.as_str() {
                    "json" => OutputFormat::Json,
                    "md" => OutputFormat::Markdown,
                    other => {
                        return Err(format!("unknown format '{other}' (expected json|md)\n{USAGE}"))
                    }
                };
            }
            "--debug" => debug = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown option '{other}'\n{USAGE}"))
            }
            // The first bare argument is the input file.
            _ if file.is_none() => file = Some(arg),
            _ => return Err(format!("unexpected argument '{arg}'\n{USAGE}")),
        }
    }

    let file = file.ok_or_else(|| format!("missing <FILE>\n{USAGE}"))?;
    Ok(Args { file, format, debug })
}

/// Reconstruct a page's text by concatenating chars in order, joining pages with
/// a Markdown horizontal rule. This is intentionally a flat text dump.
fn render_markdown(doc: &Document) -> String {
    // Use the core output layer (# headings, GitHub tables, reading order).
    let mut out = pdfmuse_core::to_markdown(doc);
    out.push('\n');
    out
}

/// Write per-page diagnostics to stderr: char counts and a peek at the first few
/// glyphs (bbox + text) of each page. Kept off stdout so it never pollutes the
/// machine-readable output.
fn print_debug(doc: &Document) {
    /// How many leading chars to show per page.
    const PREVIEW: usize = 5;

    eprintln!("debug: {} page(s), {} warning(s)", doc.pages.len(), doc.warnings.len());
    for page in &doc.pages {
        eprintln!(
            "  page {} ({:.1}x{:.1} pt): {} char(s)",
            page.index,
            page.width,
            page.height,
            page.chars.len()
        );
        for ch in page.chars.iter().take(PREVIEW) {
            let b = &ch.bbox;
            // Escape control chars (e.g. newlines) so one glyph stays on one line.
            let mut text = String::new();
            for c in ch.text.chars() {
                let _ = write!(text, "{}", c.escape_debug());
            }
            eprintln!(
                "    [{:.1},{:.1} {:.1},{:.1}] {:?} \"{}\"",
                b.x0, b.y0, b.x1, b.y1, ch.font.name, text
            );
        }
        if page.chars.len() > PREVIEW {
            eprintln!("    … {} more", page.chars.len() - PREVIEW);
        }
    }
}

/// One-line usage banner reused across argument errors.
const USAGE: &str = "usage: pdfmuse parse <FILE> [--format json|md] [--debug]";
