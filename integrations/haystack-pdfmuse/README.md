# pdfmuse-haystack

[Haystack](https://haystack.deepset.ai) converter for [**pdfmuse**](https://github.com/casperkwok/pdfmuse) —
a **deterministic** PDF/DOCX parser for RAG. Same file in → same Document out.

```bash
pip install pdfmuse-haystack
```

## Usage

```python
from pdfmuse_haystack import PdfmuseConverter

converter = PdfmuseConverter(mode="markdown")   # or "text" (default)
docs = converter.run(sources=["report.pdf"])["documents"]
```

In a pipeline:

```python
from haystack import Pipeline
from pdfmuse_haystack import PdfmuseConverter

pipe = Pipeline()
pipe.add_component("converter", PdfmuseConverter(mode="text"))
# ... connect to a splitter / embedder / writer ...
```

## Modes

- `"text"` *(default)* — plain reading-order text (fast path).
- `"markdown"` — structured Markdown (headings + tables).

Extracts text with exact coordinates, tables and structure; no probabilistic models
in the core path, so your index is reproducible run-to-run. Scanned/image-only pages
surface a `NeedsOcr` warning (OCR is a pluggable backend, kept out of the core).

MIT OR Apache-2.0 · part of the [pdfmuse](https://github.com/casperkwok/pdfmuse) project.
