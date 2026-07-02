<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/pdfmuse-logo-dark.svg">
    <img alt="pdfmuse" src="assets/pdfmuse-logo.svg" width="340">
  </picture>
</p>

<p align="center"><strong>English</strong> · <a href="README.zh-CN.md">中文</a></p>

**Deterministic PDF/DOCX parser for RAG / LLMs** — one Rust core, with Python, Node & WASM bindings that produce **byte-identical** output.

pdfmuse is a **precision pre-layer for AI/RAG**: it extracts everything a file actually contains — text with exact coordinates, fonts, vector rules, tables, links — fast, robustly, and identically across every binding. It stops cleanly at the ML boundary: OCR and visual layout inference are left to a pluggable backend, so the core stays deterministic with **zero ML dependencies**. It is **not** another probabilistic vision model.

## Why pdfmuse

| | |
|---|---|
| **Complete** | Keeps the finest-grained chars + coordinates; never silently drops content. |
| **Fast** | Zero-copy streaming Rust core with a custom O(1) object parser + content tokenizer and per-page parallelism. |
| **Robust** | A broken page/object never sinks the doc — returns structured errors, never panics (fuzz-tested). |
| **Deterministic** | Same input → same output. No probabilistic models, no time/RNG in the core path. |
| **Consistent** | Python / Node / WASM call one Rust core; output is **byte-identical** (CI-enforced). |
| **CJK first-class** | CID/Type0 fonts + CMap/ToUnicode in the main path; compatibility codepoints NFKC-normalized for clean search. |

## Performance

Measured on 22 real-world PDFs (resumes, reports, invoices; median of 7 runs, core-to-core, each returning a string):

| vs | result |
|---|---|
| **pdfplumber** (Python, common RAG choice) | **~28–39× faster** |
| **PyMuPDF** (mature C library) | **~4× faster** (wins every file in the sample) |

Text output is preserved (median 100% non-whitespace coverage vs PyMuPDF). See [`benches/`](benches) and `examples/visual_check.py`. Numbers are hardware-dependent — reproduce locally.

## Install

```bash
# Rust
cargo add pdfmuse-core
# Python (abi3 wheels)
pip install pdfmuse
# Node
npm install @pdfmuse/node   # native binding
# WASM (browser)
npm install @pdfmuse/core   # or build: wasm-pack build crates/pdfmuse-wasm --target web
```

## Usage

**CLI** (debug/inspection):
```bash
pdfmuse parse report.pdf --format md      # structured Markdown (headings, tables)
pdfmuse parse report.pdf --format json    # full IR (chars, bboxes, blocks, warnings)
```

**Rust**:
```rust
let data = std::fs::read("report.pdf")?;
let doc = pdfmuse_core::parse(&data, None)?;                 // auto-detect PDF/DOCX
for page in &doc.pages {
    for ch in &page.chars { /* ch.text, ch.bbox {x0,y0,x1,y1}, ch.size */ }
}
let md = pdfmuse_core::to_markdown(&doc);
let chunks = pdfmuse_core::chunk(&doc);                      // RAG chunks + {page, bbox, heading_path}
```

**Python**:
```python
import pdfmuse
doc = pdfmuse.parse(open("report.pdf", "rb").read())
text = "".join(c.text for pg in doc.pages for c in pg.chars)
```

**Node**:
```js
const { parse_buffer } = require("@pdfmuse/node");
const doc = JSON.parse(parse_buffer(fs.readFileSync("report.pdf")));
```

**WASM** (browser — digital PDFs; scanned pages return a `NeedsOcr` warning to hand off server-side):
```js
import init, { parse } from "@pdfmuse/core";
await init();
const doc = JSON.parse(parse(new Uint8Array(bytes)));
```

## Scope boundary

**In the core (deterministic):** text + coordinates/font/size/color · vector rules & rects · line/paragraph/column clustering · ruled & whitespace-aligned table reconstruction · full DOCX structure · JSON / Markdown / RAG-chunk output.

**Out of the core (pluggable `VisionBackend`):** scanned-page OCR · borderless-table structure recognition · heading/body/caption classification. Text-less (scanned/image) pages are flagged `NeedsOcr` and left for a backend — see [`docs/adr/0001-pdf-engine-strategy.md`](docs/adr/0001-pdf-engine-strategy.md).

Guarding this boundary is what keeps pdfmuse fast, stable, and distinct from vision models.

## Layout

```
crates/
  pdfmuse-core/     pure-Rust core: PDF/DOCX → unified IR (parser, tokenizer, layout, output)
  pdfmuse-python/   PyO3 (abi3) binding
  pdfmuse-node/     napi-rs binding
  pdfmuse-wasm/     wasm-bindgen binding
  pdfmuse-cli/      debug CLI (`pdfmuse`)
tests/{corpus,snapshots}   golden corpus + insta snapshots
tests/parity/              cross-binding byte-identical gate (Python == Node == WASM)
examples/visual_check.py   render original ↔ coordinate reconstruction for QA
fuzz/                      cargo-fuzz targets (never-panic)
```

## Testing gates

- **Snapshot tests** (`insta` + `tests/corpus`)
- **Cross-binding parity CI** — Python/Node/WASM output byte-identical (a red gate blocks merge)
- **Robustness** — mutated/garbage input never panics (`tests/robustness.rs` + `fuzz/`)
- **CJK correctness** suite

## Status

Core is feature-complete (milestones M0–M4 + real-world hardening M4.5): PDF + DOCX → unified IR → JSON / Markdown / RAG chunks, three byte-identical bindings, encryption, CJK. Currently in **M5 · polish & release**. Roadmap and tasks live in Linear (project **pdfmuse**).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
