#!/usr/bin/env python3
"""Cross-binding parity gate — the enforcement point for "byte-identical output".

Python and Node both call the same Rust core through the same serde path, so their
raw IR JSON must match **exactly**. Any divergence means a binding drifted, which
is a merge blocker. This is how pdfmuse's core promise ("one core, identical output
everywhere") becomes a red line CI guards on every change.

Run: `python tests/parity/check.py` (needs the Python binding installed via
`maturin develop` and the Node addon built via `npm run build:native`).
"""

import pathlib
import subprocess
import sys

import pdfmuse._native as native

ROOT = pathlib.Path(__file__).resolve().parents[2]
CORPUS = ROOT / "tests" / "corpus"
FILES = ["hello.pdf", "table.pdf", "cjk.pdf", "sample.docx"]


def main() -> int:
    failed = False
    for name in FILES:
        data = (CORPUS / name).read_bytes()
        py = native.parse_bytes(data, None)
        node = subprocess.check_output(
            ["node", str(ROOT / "tests" / "parity" / "node_dump.js"), str(CORPUS / name)]
        ).decode("utf-8")

        if py == node:
            print(f"OK   {name}: {len(py)} bytes identical")
        else:
            failed = True
            print(f"FAIL {name}: python {len(py)}B vs node {len(node)}B")
            for i, (a, b) in enumerate(zip(py, node)):
                if a != b:
                    lo = max(0, i - 25)
                    print(f"     first diff @ {i}: {py[lo:i + 25]!r} vs {node[lo:i + 25]!r}")
                    break

    if failed:
        print("PARITY GATE: FAILED", file=sys.stderr)
        return 1
    print("PARITY GATE: PASSED — Python and Node are byte-identical")
    return 0


if __name__ == "__main__":
    sys.exit(main())
