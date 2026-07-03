"""Haystack converter for **pdfmuse** — a deterministic PDF/DOCX parser.

    from pdfmuse_haystack import PdfmuseConverter

    converter = PdfmuseConverter(mode="markdown")
    docs = converter.run(sources=["report.pdf"])["documents"]

Deterministic: the same file always yields the same Document, so your index is
reproducible run-to-run. Scanned/image-only pages surface a NeedsOcr warning.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any, Dict, List, Optional, Union

from haystack import Document, component, default_from_dict, default_to_dict
from haystack.components.converters.utils import get_bytestream_from_source, normalize_metadata
from haystack.dataclasses import ByteStream

__all__ = ["PdfmuseConverter"]
__version__ = "0.1.0"


def _to_str(data: bytes, mode: str) -> str:
    import pdfmuse

    if mode == "markdown":
        return pdfmuse.to_markdown(data)
    return pdfmuse.to_text(data)


@component
class PdfmuseConverter:
    """Convert PDF/DOCX files to Haystack ``Document``s via pdfmuse.

    Args:
        mode: ``"text"`` (plain reading-order text, default) or ``"markdown"``
            (headings + tables).
    """

    def __init__(self, mode: str = "text") -> None:
        if mode not in ("text", "markdown"):
            raise ValueError("mode must be 'text' or 'markdown'")
        self.mode = mode

    def to_dict(self) -> Dict[str, Any]:
        return default_to_dict(self, mode=self.mode)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "PdfmuseConverter":
        return default_from_dict(cls, data)

    @component.output_types(documents=List[Document])
    def run(
        self,
        sources: List[Union[str, Path, ByteStream]],
        meta: Optional[Union[Dict[str, Any], List[Dict[str, Any]]]] = None,
    ):
        documents: List[Document] = []
        meta_list = normalize_metadata(meta, sources_count=len(sources))
        for source, md in zip(sources, meta_list):
            try:
                bytestream = get_bytestream_from_source(source)
            except Exception:
                continue
            try:
                content = _to_str(bytestream.data, self.mode)
            except Exception:
                continue
            merged = {**bytestream.meta, **md}
            documents.append(Document(content=content, meta=merged))
        return {"documents": documents}
