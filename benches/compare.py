#!/usr/bin/env python3
"""Compare pdfmuse against pdfplumber and PyMuPDF on real PDFs.

Fair, core-to-core: each tool is timed returning a string (pdfmuse via its raw
native binding, not the deserializing wrapper). Point it at directories of your
own PDFs — none are bundled.

    pip install pdfmuse pdfplumber pymupdf
    python benches/compare.py --dir ~/pdfs --runs 7

pdfplumber/PyMuPDF are optional; missing ones are skipped.
"""
import argparse, glob, os, statistics, time, io


def is_pdf(p):
    try:
        with open(p, "rb") as f:
            return f.read(5) == b"%PDF-"
    except OSError:
        return False


def median_ms(fn, data, runs):
    ts = []
    for _ in range(runs):
        t0 = time.perf_counter()
        fn(data)
        ts.append((time.perf_counter() - t0) * 1000)
    return statistics.median(ts)


def geomean(xs):
    xs = [x for x in xs if x and x > 0]
    return statistics.geometric_mean(xs) if xs else float("nan")


def main():
    ap = argparse.ArgumentParser(description="pdfmuse vs pdfplumber/PyMuPDF")
    ap.add_argument("--dir", action="append", default=[], help="directory of PDFs (repeatable)")
    ap.add_argument("--runs", type=int, default=7)
    ap.add_argument("--limit", type=int, default=30)
    args = ap.parse_args()

    import pdfmuse._native as native
    tools = [("pdfmuse", lambda d: native.parse_bytes(d, None))]
    try:
        import fitz
        tools.append(("pymupdf", lambda d: "\n".join(p.get_text() for p in fitz.open(stream=d, filetype="pdf"))))
    except ImportError:
        pass
    try:
        import pdfplumber
        def pp(d):
            with pdfplumber.open(io.BytesIO(d)) as pdf:
                return "\n".join((p.extract_text() or "") for p in pdf.pages)
        tools.append(("pdfplumber", pp))
    except ImportError:
        pass

    files = []
    for d in args.dir:
        files += [f for f in glob.glob(os.path.join(os.path.expanduser(d), "*.pdf")) if is_pdf(f)]
    files = files[: args.limit]
    if not files:
        raise SystemExit("no PDFs — pass --dir DIR")

    ratios = {name: [] for name, _ in tools[1:]}
    hdr = f'{"file":34}' + "".join(f"{n:>11}" for n, _ in tools)
    print(hdr + "\n" + "-" * len(hdr))
    for p in files:
        data = open(p, "rb").read()
        times = {}
        for name, fn in tools:
            try:
                times[name] = median_ms(fn, data, args.runs)
            except Exception:
                times[name] = float("nan")
        base = times["pdfmuse"]
        for name in ratios:
            if times[name] == times[name] and base:
                ratios[name].append(times[name] / base)
        row = f"{os.path.basename(p)[:33]:34}" + "".join(f"{times[n]:11.1f}" for n, _ in tools)
        print(row)
    print("-" * len(hdr))
    for name in ratios:
        g = geomean(ratios[name])
        print(f"pdfmuse is {g:.1f}x faster than {name} (geomean, {len(ratios[name])} files)")


if __name__ == "__main__":
    main()
