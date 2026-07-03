# @pdfmuse/node

Native Node.js binding for [**pdfmuse**](https://github.com/casperkwok/pdfmuse) — a
deterministic PDF/DOCX parser for RAG / LLMs. One Rust core, byte-identical output
across Python / Node / WASM.

```bash
npm i @pdfmuse/node
```

## Usage

```js
const { toText, toMarkdown, parse } = require("@pdfmuse/node");
const fs = require("fs");
const data = fs.readFileSync("report.pdf");

const text = toText(data);        // plain reading-order text — fast path
const md   = toMarkdown(data);    // structured Markdown (headings, tables)
const doc  = parse(data);         // full IR: doc.pages[i].chars/blocks with bboxes
```

Extracts text with exact coordinates, tables and structure; deterministic (no ML in
the core path). Scanned/image-only pages return a `NeedsOcr` warning. `.docx` works
too, auto-detected from the bytes.

- **Try it in the browser:** https://casperkwok.github.io/pdfmuse/
- **Docs & source:** https://github.com/casperkwok/pdfmuse

MIT OR Apache-2.0
