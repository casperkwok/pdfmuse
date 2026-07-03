"""pdfmuse — deterministic PDF/DOCX parser for RAG / LLMs.

Thin typed wrapper over the Rust core. The native extension returns the IR as
JSON; here we deserialize it into dataclasses. Typing deepens as the IR grows —
for now nested structures (chars, blocks, …) are left as plain dicts/lists.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any, Optional

from . import _native

__all__ = ["parse", "to_text", "to_markdown", "Document", "Page"]


@dataclass
class Page:
    index: int
    width: float
    height: float
    rotation: int
    chars: list[Any]
    lines: list[Any]
    blocks: list[Any]
    rects: list[Any]
    rules: list[Any]
    images: list[Any]
    links: list[Any]


@dataclass
class Document:
    source: str
    metadata: dict[str, Any]
    pages: list[Page]
    outline: list[Any]
    warnings: list[Any]


def parse(data: bytes, fmt: Optional[str] = None) -> Document:
    """Parse PDF/DOCX bytes into a :class:`Document`.

    ``fmt`` forces a format (``"pdf"``/``"docx"``); ``None`` auto-detects from
    magic bytes.
    """
    raw = json.loads(_native.parse_bytes(bytes(data), fmt))
    pages = [Page(**p) for p in raw["pages"]]
    return Document(
        source=raw["source"],
        metadata=raw["metadata"],
        pages=pages,
        outline=raw["outline"],
        warnings=raw["warnings"],
    )


def to_text(data: bytes, fmt: Optional[str] = None, drop_boilerplate: bool = False) -> str:
    """Parse and return plain reading-order text.

    Faster than ``parse`` when you only need text: the Rust core returns one
    string, so nothing is deserialized on the Python side (no ``json.loads`` of
    the full IR). ``fmt`` forces ``"pdf"``/``"docx"``; ``None`` auto-detects.
    ``drop_boilerplate=True`` strips running headers/footers (page numbers,
    repeated titles) detected across pages.
    """
    return _native.text_bytes(bytes(data), fmt, drop_boilerplate)


def to_markdown(data: bytes, fmt: Optional[str] = None, drop_boilerplate: bool = False) -> str:
    """Parse and return structured Markdown (headings + tables), returned as one
    string from the Rust core — same speed benefit as :func:`to_text`.

    ``drop_boilerplate=True`` strips running headers/footers first.
    """
    return _native.markdown_bytes(bytes(data), fmt, drop_boilerplate)
