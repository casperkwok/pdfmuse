#!/usr/bin/env python3
"""Benchmark pdfmuse against PyMuPDF and pdfplumber on real PDFs — speed + coverage.

Fair, apples-to-apples: every tool is timed doing **text extraction, returning a
string**. pdfmuse is timed two ways:

  * ``pdfmuse``      — ``to_text`` (plain reading-order text): the like-for-like
                       comparison against ``get_text`` / ``extract_text``.
  * ``pdfmuse+ir``   — full parse to the IR + JSON (chars, bboxes, tables, blocks):
                       far more output, shown to prove it's still competitive.

Method: per file, W warm-up calls (discarded) then N timed calls; we take the
median (and report p90 spread). A file counts only if PyMuPDF and pdfmuse both
succeed. We print tool versions + platform, the per-file table, and aggregates:
geometric-mean speedup, **win rate**, and the worst case (honest about losses).

Reproduce the arXiv corpus these numbers ship for:

    python benches/fetch_corpus.py --out /tmp/corpus      # downloads from manifest
    pip install "pdfmuse==0.1.8" "pymupdf==1.28.0" "pdfplumber==0.11.10"
    python benches/compare.py --dir /tmp/corpus

Or point --dir at your own PDFs. pdfplumber is ~20-40x slower, so it defaults to
fewer runs; pass --pp-runs to change.
"""
import argparse, glob, io, os, platform, statistics, time


def is_pdf(p):
    try:
        with open(p, "rb") as f:
            return f.read(5) == b"%PDF-"
    except OSError:
        return False


def timed(fn, data, runs, warmup):
    """Median and p90 of `runs` timed calls (ms), after `warmup` discarded calls."""
    for _ in range(warmup):
        fn(data)
    ts = []
    for _ in range(runs):
        t0 = time.perf_counter()
        fn(data)
        ts.append((time.perf_counter() - t0) * 1000)
    ts.sort()
    p90 = ts[min(len(ts) - 1, int(round(0.9 * (len(ts) - 1))))]
    return statistics.median(ts), p90


def nonspace(s):
    return "".join(s.split())


def versions():
    line = f"platform : {platform.platform()}  |  python {platform.python_version()}"
    import pdfmuse
    try:
        import importlib.metadata as _md
        _pv = _md.version("pdfmuse")
    except Exception:
        _pv = getattr(pdfmuse, "__version__", "?")
    parts = [f"pdfmuse {_pv}"]
    try:
        import fitz
        parts.append(f"pymupdf {fitz.pymupdf_version}")
    except Exception:
        pass
    try:
        import pdfplumber
        parts.append(f"pdfplumber {pdfplumber.__version__}")
    except Exception:
        pass
    return line + "\n" + "tools    : " + "  |  ".join(parts)


def main():
    ap = argparse.ArgumentParser(description="pdfmuse vs PyMuPDF/pdfplumber")
    ap.add_argument("--dir", action="append", default=[], help="directory of PDFs (repeatable)")
    ap.add_argument("--runs", type=int, default=7, help="timed runs for fast tools")
    ap.add_argument("--warmup", type=int, default=2)
    ap.add_argument("--pp-runs", type=int, default=2, help="timed runs for pdfplumber (slow)")
    ap.add_argument("--limit", type=int, default=1000)
    args = ap.parse_args()

    import pdfmuse._native as native
    tools = [
        ("pdfmuse", lambda d: native.text_bytes(d, None), args.runs),
        ("pdfmuse+ir", lambda d: native.parse_bytes(d, None), args.runs),
    ]
    have_mupdf = False
    try:
        import fitz
        tools.append(("pymupdf", lambda d: "\n".join(p.get_text() for p in fitz.open(stream=d, filetype="pdf")), args.runs))
        have_mupdf = True
    except ImportError:
        pass
    try:
        import pdfplumber
        def pp(d):
            with pdfplumber.open(io.BytesIO(d)) as pdf:
                return "\n".join((p.extract_text() or "") for p in pdf.pages)
        tools.append(("pdfplumber", pp, args.pp_runs))
    except ImportError:
        pass

    files = []
    for d in args.dir:
        files += [f for f in sorted(glob.glob(os.path.join(os.path.expanduser(d), "*.pdf"))) if is_pdf(f)]
    files = files[: args.limit]
    if not files:
        raise SystemExit("no PDFs — pass --dir DIR")

    print(versions())
    print(f"corpus   : {len(files)} PDFs  |  runs={args.runs} (pdfplumber={args.pp_runs}), warmup={args.warmup}\n")

    names = [n for n, _, _ in tools]
    hdr = f'{"file":26}' + "".join(f"{n:>12}" for n in names)
    print(hdr + "\n" + "-" * len(hdr))

    ratios = {n: [] for n in names if n != "pdfmuse"}
    cover = []  # pdfmuse vs pymupdf non-space char coverage
    counted = 0
    for p in files:
        data = open(p, "rb").read()
        med, texts = {}, {}
        for name, fn, r in tools:
            try:
                med[name], _ = timed(fn, data, r, args.warmup)
                if name in ("pdfmuse", "pymupdf"):
                    texts[name] = fn(data)
            except Exception:
                med[name] = float("nan")
        # Count a file only if the fair pair both succeeded.
        if med.get("pdfmuse") == med.get("pdfmuse") and (not have_mupdf or med.get("pymupdf") == med.get("pymupdf")):
            counted += 1
            base = med["pdfmuse"]
            for n in ratios:
                if med.get(n) == med.get(n) and base:
                    ratios[n].append(med[n] / base)
            if have_mupdf and texts.get("pymupdf"):
                ref = len(nonspace(texts["pymupdf"]))
                if ref:
                    cover.append(min(1.0, len(nonspace(texts.get("pdfmuse", ""))) / ref))
        print(f"{os.path.basename(p)[:25]:26}" + "".join(f"{med.get(n, float('nan')):12.2f}" for n in names))

    print("-" * len(hdr))
    print(f"\ncounted {counted}/{len(files)} files (both pdfmuse & PyMuPDF parsed)\n")
    for n in ratios:
        rs = ratios[n]
        if not rs:
            continue
        g = statistics.geometric_mean(rs)
        wins = sum(1 for r in rs if r > 1.0)
        worst = min(rs)
        tag = "faster" if g >= 1 else "SLOWER"
        print(f"vs {n:12}: {g:5.1f}x {tag} (geomean, n={len(rs)})  |  win {wins}/{len(rs)} = {100*wins/len(rs):.0f}%  |  worst {worst:.2f}x")
    if cover:
        print(f"\ncoverage    : pdfmuse keeps median {100*statistics.median(cover):.1f}% of PyMuPDF's non-space chars (n={len(cover)})")


if __name__ == "__main__":
    main()
