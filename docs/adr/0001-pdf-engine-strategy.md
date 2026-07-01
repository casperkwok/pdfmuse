# ADR 0001 — PDF engine strategy: pure-Rust main path + optional pdfium fallback

- Status: Accepted
- Date: 2026-07-01
- Related: Linear PER-36 (content.rs), PER-38 (objects.rs), PER-61 (pdfium feature)

## Context

The core value of pdfmuse is extracting text with precise coordinates, fonts, and vector graphics — fast, deterministic, and compilable to WASM. Existing Rust extractors either lose coordinates/CJK (`lopdf::extract_text` is too coarse) or wrap a C engine that can't target WASM and bloats the binary.

## Decision

Use a **dual-engine strategy, switched at compile time by feature**:

- **Main path (default, pure Rust):** `lopdf` for the object tree / xref / object streams / encryption, plus a **self-written content-stream interpreter** (`pdf/content.rs`) that walks the operators (`BT/ET`, `Tj/TJ`, `Tf`, `Td/TD/Tm`, …), maintains the text matrix and graphics state, and emits chars with precise bboxes. CMap/ToUnicode handled in the main path so CJK is correct from day one. This path is pure Rust, cross-compiles, and builds to WASM.
- **Fallback path (optional `pdfium` feature, default off):** `pdfium-render` wrapping Google's pdfium for malformed/complex PDFs. Adds ~20MB + a C dependency and cannot build to WASM, so it stays off unless the main path fails or the user opts in.

To mitigate `lopdf` silently skipping malformed objects, a validation pass (`pdf/objects.rs`) records skipped objects in `Document.warnings` rather than losing data quietly.

## Consequences

- **+** Fast, deterministic, WASM-capable default; no mandatory C/ML dependency; CJK correct in the main path.
- **+** A robust escape hatch (pdfium) for the long tail of broken PDFs, without imposing its cost on everyone.
- **−** The self-written interpreter is the hardest, highest-value work (PER-36) and must be carefully tested against a golden corpus.
- **−** Two engines can diverge; the `pdfium` path is opt-in and not part of the byte-identical parity guarantee.
