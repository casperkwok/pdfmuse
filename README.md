<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/pdfmuse-logo-dark.svg">
    <img alt="pdfmuse" src="assets/pdfmuse-logo.svg" width="340">
  </picture>
</p>

<p align="center"><strong>English</strong> · <a href="README.zh-CN.md">中文</a></p>

<p align="center">
  <a href="https://crates.io/crates/pdfmuse-core"><img alt="crates.io" src="https://img.shields.io/crates/v/pdfmuse-core?logo=rust&logoColor=white&label=crates.io&color=E43716"></a>
  <a href="https://pypi.org/project/pdfmuse/"><img alt="PyPI" src="https://img.shields.io/pypi/v/pdfmuse?logo=pypi&logoColor=white&label=PyPI&color=3775A9"></a>
  <a href="https://www.npmjs.com/package/@pdfmuse/node"><img alt="npm" src="https://img.shields.io/npm/v/%40pdfmuse%2Fnode?logo=npm&label=npm&color=CB3837"></a>
  <a href="https://github.com/casperkwok/pdfmuse/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/casperkwok/pdfmuse/ci.yml?branch=main&logo=github&label=CI"></a>
  <a href="https://casperkwok.github.io/pdfmuse/"><img alt="live demo" src="https://img.shields.io/badge/demo-live-6E56CF?logo=webassembly&logoColor=white"></a>
  <img alt="license" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue">
</p>

<p align="center">
  <a href="https://casperkwok.github.io/pdfmuse/"><strong>▶ Live playground</strong></a> — drag a PDF, watch it parse in your browser (nothing is uploaded)
</p>

<p align="center">
  <a href="https://casperkwok.github.io/pdfmuse/"><img src="assets/pdfmuse.gif" alt="pdfmuse playground: original PDF ↔ pdfmuse reconstruction" width="760"></a>
</p>

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

Two things matter for a RAG pre-layer: speed, and whether it keeps the content. Both
are measured on a **public, reproducible corpus** — 61 arXiv papers across 8 fields
(large, dense PDFs — a deliberately hard case), so you can rerun the exact benchmark:

```bash
python benches/fetch_corpus.py --out /tmp/corpus      # the same PDFs, from a fixed manifest
pip install "pdfmuse==0.1.8" "pymupdf==1.28.0" "pdfplumber==0.11.10"
python benches/compare.py --dir /tmp/corpus
```

**Text extraction** (`to_text`, median of 7 runs after warm-up; PyMuPDF 1.28 / MuPDF 1.29, pdfplumber 0.11, macOS arm64):

| vs | speedup (geomean) | win rate | worst case |
|---|---|---|---|
| **PyMuPDF** | **~5.9× faster** | 56 / 61 (92%) | ~3× *slower* (one 3 MB paper) |
| **pdfplumber** | **~110× faster** | 61 / 61 (100%) | 10.6× |

**Honest caveat:** pdfmuse is *not* universally fastest. On the 5 largest/densest papers (2–22 MB), PyMuPDF's mature C core (MuPDF) wins — up to ~3×. pdfmuse wins the other 92%; on typical RAG docs (resumes, reports, invoices — 1–2 pages) it runs in ~1–2 ms and wins consistently. **Content is preserved:** median **100%** of PyMuPDF's non-whitespace characters (n=61).

`to_text()` / `to_markdown()` return a string straight from the Rust core (no full-IR deserialization). The full `parse()` — chars + bboxes + tables, far more than text — costs only ~2.3× the `to_text` time, still under PyMuPDF on most files. The native Node binding is ~as fast as the Rust core; WASM ~1.7×. Eyeball fidelity with `examples/visual_check.py`.

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
data = open("report.pdf", "rb").read()
text = pdfmuse.to_text(data)         # plain text — fast path (~1.3ms, no full-IR json.loads)
md = pdfmuse.to_markdown(data)       # structured Markdown — headings (PDF & DOCX) + tables
doc = pdfmuse.parse(data)            # full IR: doc.pages[i].chars/blocks with bboxes
clean = pdfmuse.to_text(data, drop_boilerplate=True)  # strip running headers/footers
```

**Node**:
```js
const { toText, toMarkdown, parse } = require("@pdfmuse/node");
const data = fs.readFileSync("report.pdf");
const text = toText(data);           // plain text — fast path
const clean = toText(data, undefined, true);  // strip running headers/footers
const doc = parse(data);             // full IR (typed Document)
```

**WASM** (browser — digital PDFs; scanned pages return a `NeedsOcr` warning to hand off server-side):
```js
import init, { to_text, parse } from "@pdfmuse/core";
await init();
const text = to_text(new Uint8Array(bytes));         // plain text
const doc = JSON.parse(parse(new Uint8Array(bytes))); // full IR
```

## Integrations

- **LangChain** — [`langchain-pdfmuse`](integrations/langchain-pdfmuse): a `PdfmuseLoader` with `single` / `page` / `elements` modes. In `elements` mode each chunk carries section-aware metadata (`heading_path`, `bbox`, `category`) — reproducible chunks for RAG.

  ```python
  from langchain_pdfmuse import PdfmuseLoader
  docs = PdfmuseLoader("report.pdf", mode="elements").load()
  ```

- **LlamaIndex** — [`llama-index-readers-pdfmuse`](integrations/llama-index-pdfmuse): a `PdfmuseReader` with the same modes and section-aware metadata.

  ```python
  from llama_index.readers.pdfmuse import PdfmuseReader
  docs = PdfmuseReader(mode="elements").load_data("report.pdf")
  ```

- **Haystack** — [`pdfmuse-haystack`](integrations/haystack-pdfmuse): a `PdfmuseConverter` component (`text` / `markdown`) for Haystack 2.x pipelines.

  ```python
  from pdfmuse_haystack import PdfmuseConverter
  docs = PdfmuseConverter(mode="markdown").run(sources=["report.pdf"])["documents"]
  ```

## Scope boundary

**In the core (deterministic):** text + coordinates/font/size/color · vector rules & rects · line/paragraph/column clustering · heading detection (font-size + numbering) · running header/footer detection + opt-in removal · ruled & whitespace-aligned table reconstruction · full DOCX structure · JSON / Markdown / RAG-chunk output.

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
