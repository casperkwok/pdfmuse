# Submitting the pdfmuse loader to the LangChain docs

Draft is ready — **you** submit it (from your own GitHub account, so the PR has a
real human owner who replies to review). This is prepared, not yet submitted.

## What LangChain accepts (verified 2026-07)

- LangChain docs moved to **Mintlify** in the **`langchain-ai/docs`** repo (not
  `langchain-ai/langchain`). Integration pages are **`.mdx`**.
- Third-party loader pages are still accepted (e.g. `docling`, `undatasio` are
  independent PyPI packages, not `langchain-*` partner packages — same as us).
- The nav (`src/docs.json`) only points at the loaders **index**, so a complete PR
  is **two files**:

## The PR (2 files)

**1. New page** → `src/oss/python/integrations/document_loaders/pdfmuse.mdx`
Copy [`pdfmuse.mdx`](./pdfmuse.mdx) from this folder.

**2. List it** → `src/oss/python/integrations/document_loaders/index.mdx`
Add a row to the general loaders table, and to the **PDF** loaders table
(match the existing format — `| [Name](path) | description | Package |`):

```
| [pdfmuse](/oss/integrations/document_loaders/pdfmuse) | Deterministic PDF/DOCX loader; per-block chunks with page / heading_path / bbox metadata for RAG | Package |
```

## Steps

```bash
# fork langchain-ai/docs on GitHub, then:
git clone https://github.com/<you>/docs langchain-docs && cd langchain-docs
git checkout -b docs-pdfmuse-loader
# add the two changes above
npx mint dev        # local Mintlify preview (see the repo README "Contribute")
# verify the page renders and appears under Integrations → Document loaders
git commit -am "docs: add pdfmuse document loader integration"
git push origin docs-pdfmuse-loader
# open the PR against langchain-ai/docs:main
```

**PR title:** `docs: add pdfmuse document loader integration`

**PR body (human, no AI signature):**
> Adds an integration docs page for `langchain-pdfmuse`, a document loader for
> [pdfmuse](https://github.com/casperkwok/pdfmuse) — a deterministic PDF/DOCX
> parser aimed at RAG. It runs locally (no API, no ML deps in the core), extracts
> text with exact coordinates + tables, and in `elements` mode emits per-block
> chunks with `page` / `heading_path` / `bbox` metadata. Package is on PyPI
> (`langchain-pdfmuse`). Previewed locally with `mint dev`.

## Etiquette (so it lands and doesn't read as a drive-by)

- Submit under **your** account; reply to review comments yourself, promptly.
- Keep strictly to the template (the `.mdx` here already does).
- **Timing:** stronger once the package has a little traction (a few stars /
  downloads). Consider submitting after the Show HN / Reddit posts, not before.
