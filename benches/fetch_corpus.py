#!/usr/bin/env python3
"""Download the public benchmark corpus so anyone can reproduce the numbers.

Reads ``benches/corpus-manifest.txt`` (a fixed list of arXiv IDs) and fetches each
paper's PDF into ``--out``. arXiv PDFs are public and stable, so the exact corpus
the README benchmarks on is reproducible — no cherry-picked private files.

    python benches/fetch_corpus.py --out /tmp/corpus
    python benches/compare.py --dir /tmp/corpus

Be polite to arXiv: this sleeps between requests. ~60 files ≈ a few minutes.
"""
import argparse, os, time, urllib.request

MANIFEST = os.path.join(os.path.dirname(__file__), "corpus-manifest.txt")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, help="output directory for PDFs")
    ap.add_argument("--sleep", type=float, default=3.0, help="seconds between requests (arXiv politeness)")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    ids = [ln.strip() for ln in open(MANIFEST) if ln.strip() and not ln.startswith("#")]
    ok = 0
    for aid in ids:
        fn = os.path.join(args.out, aid.replace("/", "_") + ".pdf")
        if os.path.exists(fn) and os.path.getsize(fn) > 1000:
            ok += 1
            continue
        try:
            req = urllib.request.Request(
                f"https://arxiv.org/pdf/{aid}",
                headers={"User-Agent": "pdfmuse-bench/1.0 (https://github.com/casperkwok/pdfmuse)"},
            )
            data = urllib.request.urlopen(req, timeout=60).read()
            if data[:5] == b"%PDF-":
                open(fn, "wb").write(data)
                ok += 1
            else:
                print("  not a PDF:", aid)
        except Exception as e:
            print("  skip", aid, str(e)[:60])
        time.sleep(args.sleep)
    print(f"fetched {ok}/{len(ids)} PDFs into {args.out}")


if __name__ == "__main__":
    main()
