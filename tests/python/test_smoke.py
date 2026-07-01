"""M0 end-to-end smoke test for the Python binding.

Requires the binding installed into the active venv:
    maturin develop
Run:
    pytest tests/python
"""

from pathlib import Path

import pytest

import pdfmuse

FIXTURE = Path(__file__).resolve().parents[2] / "tests" / "corpus" / "hello.pdf"


def test_parse_returns_document_with_text():
    doc = pdfmuse.parse(FIXTURE.read_bytes())
    assert doc.source == "Pdf"
    assert len(doc.pages) == 1

    text = " ".join(
        blk["Paragraph"]["text"]
        for page in doc.pages
        for blk in page.blocks
        if isinstance(blk, dict) and "Paragraph" in blk
    )
    assert "Hello pdfmuse" in text


def test_unknown_bytes_raise_valueerror():
    with pytest.raises(ValueError):
        pdfmuse.parse(b"not a document")
