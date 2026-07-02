"""LlamaIndex reader for **pdfmuse** — a deterministic PDF/DOCX parser.

    from llama_index.readers.pdfmuse import PdfmuseReader

    docs = PdfmuseReader(mode="elements").load_data("report.pdf")

Metadata values are kept to simple types (str / int) so the Documents index
cleanly in LlamaIndex vector stores.
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import List, Optional

from llama_index.core.readers.base import BaseReader
from llama_index.core.schema import Document

__all__ = ["PdfmuseReader"]


def _parse(data: bytes, fmt: Optional[str]) -> dict:
    import pdfmuse

    fn = getattr(pdfmuse, "parse_bytes", None)
    if fn is None:  # native fn lives under the compiled submodule
        from pdfmuse import _native

        fn = _native.parse_bytes
    return json.loads(fn(data, fmt))


def _block(b: dict):
    """Return (category, text, heading_level, bbox_dict)."""
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
    if not parts:
        parts = ["".join(c.get("text", "") for c in page.get("chars", []))]
    return "\n\n".join(p for p in parts if p)


def _bbox_str(bbox: Optional[dict]) -> str:
    if not bbox:
        return ""
    return f"{bbox['x0']:.0f},{bbox['y0']:.0f},{bbox['x1']:.0f},{bbox['y1']:.0f}"


class PdfmuseReader(BaseReader):
    """Read a PDF or DOCX into LlamaIndex ``Document`` objects via pdfmuse.

    Args:
        mode: ``"single"`` | ``"page"`` | ``"elements"``.
        fmt: force a format (``"pdf"`` / ``"docx"``); ``None`` auto-detects.
    """

    def __init__(self, mode: str = "page", fmt: Optional[str] = None) -> None:
        super().__init__()
        if mode not in ("single", "page", "elements"):
            raise ValueError("mode must be 'single', 'page' or 'elements'")
        self._mode = mode
        self._fmt = fmt

    def load_data(self, file, extra_info: Optional[dict] = None) -> List[Document]:
        doc = _parse(Path(file).read_bytes(), self._fmt)
        pages = doc.get("pages", [])
        base = {"source": str(file), "source_kind": doc.get("source") or ""}
        if extra_info:
            base.update(extra_info)

        out: List[Document] = []
        if self._mode == "single":
            text = "\n\n".join(_page_text(p) for p in pages)
            out.append(Document(text=text, metadata={**base, "total_pages": len(pages)}))
            return out

        if self._mode == "page":
            for p in pages:
                out.append(
                    Document(
                        text=_page_text(p),
                        metadata={**base, "page": p.get("index", 0), "total_pages": len(pages)},
                    )
                )
            return out

        # elements — metadata flattened to simple types for clean indexing
        for p in pages:
            heading_path: List[str] = []
            for b in p.get("blocks", []):
                cat, text, lvl, bbox = _block(b)
                if not text:
                    continue
                if lvl:
                    heading_path = heading_path[: lvl - 1] + [text]
                out.append(
                    Document(
                        text=text,
                        metadata={
                            **base,
                            "page": p.get("index", 0),
                            "category": cat,
                            "heading_path": " > ".join(heading_path),
                            "bbox": _bbox_str(bbox),
                        },
                    )
                )
        return out
