# Contributing to pdfmuse

Thanks for your interest! pdfmuse is a deterministic PDF/DOCX parser — a Rust core
with Python / Node / WASM bindings that produce **byte-identical** output. Bug
reports (especially files it gets wrong), fixes, and docs are all welcome.

## Ground rules

The five principles the code is held to (see [README](README.md) and
`docs/adr/0001-pdf-engine-strategy.md`):

1. **Determinism** — same input → same output; no probabilistic models, no time/RNG
   in the core path.
2. **Zero-copy / streaming** — borrow, don't clone; process page-by-page.
3. **Cross-binding parity** — all bindings call one Rust core through one serde path;
   output is byte-identical (CI-enforced). Floats serialize at fixed precision.
4. **Graceful degradation** — a broken page/object never sinks the doc; return a
   `Result`, never `panic!`; record downgrades in `Document.warnings`.
5. **Clear ML boundary** — OCR / layout inference live behind the `VisionBackend`
   trait. The core crate has **zero ML dependencies**.

If a change moves the core toward a layout/vision model, it belongs in a backend.

## Dev loop

```bash
cargo test                                 # unit + snapshot + robustness (core + cli)
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p pdfmuse-cli -- parse <file> --format md   # eyeball output

# bindings are cdylibs built by their packaging tools, NOT `cargo build`:
maturin develop                            # Python
napi build --platform                      # Node
wasm-pack build crates/pdfmuse-wasm --target web
```

## Testing gates (must stay green)

- Snapshot tests (`insta` + `tests/corpus/`).
- **Cross-binding byte-identical parity** CI (`tests/parity/`) — a red parity gate
  blocks merge.
- `cargo-fuzz` never-panic robustness, and a CJK-correctness suite.

When you change parsing output, add or update a fixture in `tests/corpus/` and its
snapshot. Reporting a file that parses wrong? A small reproducing PDF is gold.

## Pull requests

- Branch per change; keep PRs focused.
- Run `cargo test` + `cargo clippy` before pushing.
- Commits sign the author only — please don't add `Co-Authored-By` trailers.

Dual-licensed **MIT OR Apache-2.0**; contributions are accepted under the same terms.
