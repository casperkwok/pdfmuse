#!/usr/bin/env python3
"""Cross-binding parity gate — the enforcement point for "byte-identical output".

Python, Node, and WASM all call the same Rust core through the same serde path, so
their raw IR JSON must match **exactly**. Any divergence means a binding drifted,
which is a merge blocker. This is how pdfmuse's core promise ("one core, identical
output everywhere") becomes a red line CI guards on every change.

Prerequisites:
  - Python binding installed via `maturin develop`.
  - Node addon built via `npm run build:native` (in bindings/node).
  - WASM package built via `wasm-pack build crates/pdfmuse-wasm --target nodejs --out-dir pkg`.
Run: `python tests/parity/check.py`.
"""

import pathlib
import subprocess
import sys

import pdfmuse._native as native

ROOT = pathlib.Path(__file__).resolve().parents[2]
CORPUS = ROOT / "tests" / "corpus"
PARITY = ROOT / "tests" / "parity"
FILES = ["hello.pdf", "table.pdf", "cjk.pdf", "sample.docx"]


def dump(script: str, path: pathlib.Path) -> str:
    """Run a Node dumper (native addon or WASM) and return its raw JSON output."""
    return subprocess.check_output(["node", str(PARITY / script), str(path)]).decode("utf-8")


def main() -> int:
    failed = False
    for name in FILES:
        data = (CORPUS / name).read_bytes()
        outs = {
            "python": native.parse_bytes(data, None),
            "node": dump("node_dump.js", CORPUS / name),
            "wasm": dump("wasm_dump.js", CORPUS / name),
        }

        if len(set(outs.values())) == 1:
            print(f"OK   {name}: {len(outs['python'])} bytes identical across python/node/wasm")
        else:
            failed = True
            sizes = ", ".join(f"{k}={len(v)}B" for k, v in outs.items())
            print(f"FAIL {name}: {sizes}")
            ref = outs["python"]
            for other, value in outs.items():
                if value != ref:
                    for i, (a, b) in enumerate(zip(ref, value)):
                        if a != b:
                            lo = max(0, i - 25)
                            print(f"     python vs {other} first diff @ {i}: {ref[lo:i + 25]!r} vs {value[lo:i + 25]!r}")
                            break

    if failed:
        print("PARITY GATE: FAILED", file=sys.stderr)
        return 1
    print("PARITY GATE: PASSED — Python, Node, and WASM are byte-identical")
    return 0


if __name__ == "__main__":
    sys.exit(main())
