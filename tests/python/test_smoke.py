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


def test_parse_returns_document_with_positioned_text():
    doc = pdfmuse.parse(FIXTURE.read_bytes())
    assert doc.source == "Pdf"
    assert len(doc.pages) == 1

    chars = doc.pages[0].chars
    text = "".join(c["text"] for c in chars)
    assert text == "Hello pdfmuse"
    # Each char has a bounding box with positive area.
    assert all(
        c["bbox"]["x1"] > c["bbox"]["x0"] and c["bbox"]["y1"] > c["bbox"]["y0"]
        for c in chars
    )


def test_unknown_bytes_raise_valueerror():
    with pytest.raises(ValueError):
        pdfmuse.parse(b"not a document")
