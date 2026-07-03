# @pdfmuse/core

Browser (WASM) build of [**pdfmuse**](https://github.com/casperkwok/pdfmuse) — a
deterministic PDF/DOCX parser for RAG / LLMs. Runs entirely in the browser; the file
never leaves the tab.

```bash
npm i @pdfmuse/core
```

## Usage

```js
import init, { to_text, parse } from "@pdfmuse/core";
await init();

const bytes = new Uint8Array(await file.arrayBuffer());
const text  = to_text(bytes);                 // plain reading-order text
const doc   = JSON.parse(parse(bytes));        // full IR: chars/blocks with bboxes
```

Extracts text with exact coordinates, tables and structure; deterministic, byte-identical
to the Python/Node/Rust bindings. Scanned/image-only pages return a `NeedsOcr` warning to
hand off server-side.

- **Live playground:** https://casperkwok.github.io/pdfmuse/
- **Docs & source:** https://github.com/casperkwok/pdfmuse

MIT OR Apache-2.0
