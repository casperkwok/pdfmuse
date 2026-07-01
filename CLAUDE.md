# CLAUDE.md — pdfmuse

Guardrails for anyone (human or AI) working in this repo. Read before editing.

## What this is

A deterministic PDF/DOCX parser — Rust core + Python/Node/WASM bindings — positioned as a **precision pre-layer for AI/RAG**, not a probabilistic vision model. Full plan: Linear project **pdfmuse** (milestones M0–M5); source docs in `~/p-brain/work/products/pdfmuse/`.

## Five principles (resolve any disagreement by returning here)

1. **Determinism first** — same input → same output; no probabilistic models in the core path; no `Math.random`/time in logic.
2. **Zero-copy & streaming** — borrow, don't clone; process large files page-by-page.
3. **Cross-binding parity** — all bindings call one Rust core and serialize through one serde path → byte-identical output. Floats serialize at fixed precision.
4. **Graceful degradation** — a broken page/object never sinks the doc; return `Result`, never `panic!`; record downgrades in `Document.warnings`.
5. **Clear boundary** — OCR / borderless-table semantics / classification go through a pluggable `VisionBackend` trait. The core crate has **zero ML dependencies**.

## Scope discipline (the make-or-break rule)

Before adding anything to the core, ask: *does this move us toward a layout model?* If yes, it belongs in a backend, not the core. Keep the in-core vs backend boundary (see README + ADR 0001) intact — drifting into layout modeling loses the fast/stable/deterministic differentiation.

## Non-obvious conventions

- **Coordinates** are normalized to **top-left origin, Y down, unit pt** everywhere (PDF's native bottom-left origin is converted at the edge). Never leak raw PDF coordinates into the IR.
- **CJK is a first-class citizen**: `Char.text` must always be Unicode (post-CMap), never a raw CID. Missing CMap → `WarningKind::MissingCMap`, not garbage.
- **Secrets**: PDF passwords must never be written to logs, URLs, or error messages.

## Workspace layout

`crates/pdfmuse-core` (value core) + thin binding shells (`-python`/`-node`/`-wasm`) + `-cli` (debug). Bindings contain **no parsing logic** — type/memory bridging only. Shared dependency versions live in the root `[workspace.dependencies]`.

## Common commands

```bash
cargo build                 # build workspace
cargo test                  # unit + snapshot tests
cargo clippy --all-targets  # lint (keep clean)
cargo run -p pdfmuse-cli -- parse <file>   # debug dump (once PER-44 lands)

# Bindings (later milestones):
maturin develop             # build+install Python binding locally (PER-34)
napi build --platform       # build Node .node (PER-48)
wasm-pack build crates/pdfmuse-wasm --target web   # WASM (PER-59)
```

## Testing gates (once built)

Snapshot tests (`insta` + `tests/corpus`), the **cross-binding byte-identical parity CI** (must stay green — it enforces principle 3), `cargo-fuzz` (never panic), and a CJK-correctness suite. A red parity gate blocks merge.

## Git / commits

- Branch names follow Linear: `casperkwok/per-<NN>-<slug>` (each PR maps to one issue).
- **Commits sign the author only — never add a `Co-Authored-By` trailer.**
- Commit or push only when explicitly asked.
