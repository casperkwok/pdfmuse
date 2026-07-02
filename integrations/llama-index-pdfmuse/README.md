# llama-index-readers-pdfmuse

LlamaIndex reader for [**pdfmuse**](https://github.com/casperkwok/pdfmuse) — a
**deterministic** PDF/DOCX parser for RAG. Same file in → same Documents out, with
exact coordinates, tables and section structure.

```bash
pip install llama-index-readers-pdfmuse
```

## Usage

```python
from llama_index.readers.pdfmuse import PdfmuseReader
from llama_index.core import VectorStoreIndex

# RAG-optimized: one Document per block, section-aware metadata
docs = PdfmuseReader(mode="elements").load_data("report.pdf")
index = VectorStoreIndex.from_documents(docs)
```

## Modes

| `mode` | Documents | Best for |
|---|---|---|
| `"single"` | the whole file as one | quick ingestion |
| `"page"` *(default)* | one per page | page-level retrieval |
| `"elements"` | one per block (heading / paragraph / table) | **RAG** — chunk with structure |

## Metadata (`elements` mode)

Kept to simple types so it indexes cleanly:

- `source`, `source_kind` (`Pdf` / `Docx`)
- `page` — 0-based page index
- `category` — `Title` · `NarrativeText` · `Table`
- `heading_path` — section breadcrumb, e.g. `"Experience > Alibaba"`
- `bbox` — `"x0,y0,x1,y1"` on the page (top-left origin, points)

`heading_path` + `bbox` let a retriever return *"which section, where on the page"* —
not just a blob of text. And because the core has no probabilistic models, your index
is reproducible run-to-run.

MIT OR Apache-2.0 · part of the [pdfmuse](https://github.com/casperkwok/pdfmuse) project.
