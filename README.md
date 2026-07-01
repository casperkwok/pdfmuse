# pdfmuse

**Deterministic PDF/DOCX parser for RAG / LLMs** — a Rust core with Python, Node & WASM bindings.

> ⚠️ Under construction. Placeholder releases are published on [crates.io](https://crates.io/crates/pdfmuse), [PyPI](https://pypi.org/project/pdfmuse/) and npm (`@pdfmuse/core`) to hold the name.

pdfmuse is a **precision pre-layer for AI/RAG**: it squeezes out everything a file actually contains — text with exact coordinates, fonts, vector rules, tables, links — fast, robustly, and **byte-identically across every binding**. It stops cleanly at the ML boundary: OCR and visual layout are left to a pluggable backend, so the core stays deterministic with zero ML dependencies. It is **not** another probabilistic vision model.

## Why pdfmuse

| | |
|---|---|
| **全 Complete** | Keeps the finest-grained chars + coordinates; never subtracts for you. |
| **快 Fast** | Zero-copy, streaming Rust core — targets 10–50× pdfplumber. |
| **稳 Robust** | A broken page/object never sinks the doc; returns structured errors, never panics. |
| **确定 Deterministic** | Same input → same output. No probabilistic models in the core path. |
| **一致 Consistent** | Python / Node / WASM share one Rust core; output is byte-identical (CI-enforced). |
| **CJK first-class** | CID-keyed fonts + CMap/ToUnicode handled in the main path from day one. |

## Scope boundary

**In the core (deterministic):** text + coordinates/font/size/color · vector rules & rects · images/links/bookmarks/form fields · line/paragraph/column clustering · ruled & whitespace-aligned table reconstruction · full DOCX structure.

**Out of the core (pluggable backend):** scanned-page OCR · irregular borderless tables · merged-cell semantic recovery · heading/body/caption classification · layout aesthetic scoring.

Guarding this boundary is what keeps pdfmuse fast, stable and distinct — see [`docs/adr/0001-pdf-engine-strategy.md`](docs/adr/0001-pdf-engine-strategy.md).

## Layout

```
crates/
  pdfmuse-core/     pure-Rust core: PDF/DOCX → unified IR
  pdfmuse-python/   PyO3 binding
  pdfmuse-node/     napi-rs binding
  pdfmuse-wasm/     wasm-bindgen binding
  pdfmuse-cli/      debug CLI
bindings/{python,node}   thin typed wrappers
tests/{corpus,snapshots} golden corpus + expected outputs
benches/                 criterion + pdfplumber comparison
```

## Status

Planning and task breakdown live in Linear (project **pdfmuse**, milestones M0–M5). This repo is at **M0 · skeleton**.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
