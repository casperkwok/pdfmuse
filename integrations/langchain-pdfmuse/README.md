# langchain-pdfmuse

LangChain document loader for [**pdfmuse**](https://github.com/casperkwok/pdfmuse) — a
**deterministic** PDF/DOCX parser for RAG. Same file in → same Documents out, with
exact coordinates, tables and section structure.

```bash
pip install langchain-pdfmuse
```

## Usage

```python
from langchain_pdfmuse import PdfmuseLoader

# one Document per page (default)
docs = PdfmuseLoader("report.pdf").load()

# RAG-optimized: one Document per block, with section-aware metadata
elements = PdfmuseLoader("report.pdf", mode="elements").load()
for e in elements:
    print(e.metadata["category"], e.metadata["heading_path"], e.metadata["bbox"])
```

## Modes

| `mode` | Documents | Best for |
|---|---|---|
| `"single"` | the whole file as one | quick ingestion |
| `"page"` *(default)* | one per page | page-level retrieval |
| `"elements"` | one per block (heading / paragraph / table) | **RAG** — chunk with structure |

## Metadata (`elements` mode)

Each `Document.metadata` carries:

- `source`, `source_kind` (`Pdf` / `Docx`)
- `page` — 0-based page index
- `category` — `Title` · `NarrativeText` · `Table`
- `heading_path` — the section breadcrumb, e.g. `["Experience", "Alibaba"]`
- `bbox` — `{x0, y0, x1, y1}` on the page (top-left origin, points)

`heading_path` + `bbox` let your retriever return *"which section, where on the page"* —
not just a blob of text.

## Why deterministic matters for RAG

Non-deterministic extractors make your chunks (and therefore embeddings and eval
results) drift between runs. pdfmuse has no probabilistic models in its core path, so
your index is reproducible. OCR / layout inference are opt-in backends, kept out of the
deterministic core — scanned pages surface a `NeedsOcr` warning instead of guessing.

MIT OR Apache-2.0 · part of the [pdfmuse](https://github.com/casperkwok/pdfmuse) project.
