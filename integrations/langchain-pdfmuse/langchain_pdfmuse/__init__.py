"""LangChain document loader for **pdfmuse** — a deterministic PDF/DOCX parser.

    from langchain_pdfmuse import PdfmuseLoader

    docs = PdfmuseLoader("report.pdf", mode="elements").load()

Modes
-----
- ``"single"``   — one Document for the whole file.
- ``"page"``     — one Document per page (default).
- ``"elements"`` — one Document per structural block (paragraph / heading / table),
  with rich metadata (``page``, ``category``, ``heading_path``, ``bbox``) — best for RAG.

Everything is deterministic: the same file always yields the same Documents.
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Iterator, List, Optional

from langchain_core.document_loaders import BaseLoader
from langchain_core.documents import Document

__all__ = ["PdfmuseLoader"]
__version__ = "0.1.0"


def _parse(data: bytes, fmt: Optional[str]) -> dict:
    """Call pdfmuse and return the parsed IR as a dict."""
    import pdfmuse  # the published `pdfmuse` wheel

    raw = getattr(pdfmuse, "parse_bytes", None)
    if raw is None:  # the native fn lives under the compiled submodule
        from pdfmuse import _native

        raw = _native.parse_bytes
    return json.loads(raw(data, fmt))


def _block(b: dict) -> tuple[str, str, Optional[int], Optional[dict]]:
    """Return (category, text, heading_level, bbox) for an IR block."""
    if "Paragraph" in b:
        p = b["Paragraph"]
        lvl = p.get("heading_level")
        return ("Title" if lvl else "NarrativeText"), (p.get("text") or ""), lvl, p.get("bbox")
    if "Table" in b:
        t = b["Table"]
        rows = t.get("rows", [])
        text = "\n".join(" | ".join((c.get("text") or "") for c in row) for row in rows)
        return "Table", text, None, t.get("bbox")
    return "Other", "", None, None


def _page_text(page: dict) -> str:
    parts = [t for b in page.get("blocks", []) if (t := _block(b)[1])]
    if not parts:  # image-only / no text layer -> fall back to raw glyph order
        parts = ["".join(c.get("text", "") for c in page.get("chars", []))]
    return "\n\n".join(p for p in parts if p)


class PdfmuseLoader(BaseLoader):
    """Load a PDF or DOCX into LangChain ``Document`` objects via pdfmuse.

    Args:
        file_path: path to a ``.pdf`` or ``.docx`` file.
        mode: ``"single"`` | ``"page"`` | ``"elements"``.
        fmt: force a format (``"pdf"`` / ``"docx"``); ``None`` auto-detects.
    """

    def __init__(self, file_path: str | Path, *, mode: str = "page", fmt: Optional[str] = None):
        if mode not in ("single", "page", "elements"):
            raise ValueError("mode must be 'single', 'page' or 'elements'")
        self.file_path = str(file_path)
        self.mode = mode
        self.fmt = fmt

    def lazy_load(self) -> Iterator[Document]:
        doc = _parse(Path(self.file_path).read_bytes(), self.fmt)
        pages = doc.get("pages", [])
        base = {"source": self.file_path, "source_kind": doc.get("source")}
        if doc.get("warnings"):
            base["warnings"] = doc["warnings"]

        if self.mode == "single":
            text = "\n\n".join(_page_text(p) for p in pages)
            yield Document(page_content=text, metadata={**base, "total_pages": len(pages)})
            return

        if self.mode == "page":
            for p in pages:
                yield Document(
                    page_content=_page_text(p),
                    metadata={**base, "page": p.get("index", 0), "total_pages": len(pages)},
                )
            return

        # elements
        for p in pages:
            heading_path: List[str] = []
            for b in p.get("blocks", []):
                cat, text, lvl, bbox = _block(b)
                if not text:
                    continue
                if lvl:
                    heading_path = heading_path[: lvl - 1] + [text]
                yield Document(
                    page_content=text,
                    metadata={
                        **base,
                        "page": p.get("index", 0),
                        "category": cat,
                        "heading_path": list(heading_path),
                        "bbox": bbox,
                    },
                )
