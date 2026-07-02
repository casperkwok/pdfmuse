#!/usr/bin/env python3
"""visual_check — eyeball pdfmuse's real parsing fidelity.

Renders, per PDF, the original page next to a reconstruction drawn *only* from
pdfmuse's extracted coordinates (chars at their bbox, rules as lines) plus the
structured Markdown output. If the reconstruction matches the original, character
/ coordinate / CJK extraction is faithful. Catches issues (collapsed lines, zero
advances, garbled reading order) that synthetic fixtures and green snapshot tests
hide — the way PER-132 and PER-133 were found.

Usage:
    python examples/visual_check.py <file.pdf> [more.pdf ...]
    python examples/visual_check.py --random 5 --dir ~/Downloads --dir ~/some/pdfs
    python examples/visual_check.py --random 3 --dir DIR --seed 42 --no-open

Original-page rendering needs PyMuPDF (`pip install pymupdf`); without it the
reconstruction + Markdown still render.
"""
import argparse, base64, glob, hashlib, html, json, os, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_bin():
    for cand in ("target/release/pdfmuse", "target/debug/pdfmuse"):
        p = os.path.join(ROOT, cand)
        if os.path.exists(p):
            return p
    sys.exit("pdfmuse binary not found — run: cargo build --release -p pdfmuse-cli")


def is_pdf(p):
    try:
        with open(p, "rb") as f:
            return f.read(5) == b"%PDF-"
    except OSError:
        return False


def pick_random(dirs, n, seed):
    files = []
    for d in dirs:
        files += [f for f in glob.glob(os.path.join(os.path.expanduser(d), "*.pdf")) if is_pdf(f)]
    files.sort(key=lambda p: hashlib.md5(f"{seed}:{p}".encode()).hexdigest())
    return files[:n]


def esc(s):
    return html.escape(s, quote=True)


def svg(page):
    w, h = page["width"], page["height"]
    els = [f'<rect width="{w:.0f}" height="{h:.0f}" fill="#fff"/>']
    for r in page["rules"]:
        els.append(f'<line x1="{r["x0"]:.1f}" y1="{r["y0"]:.1f}" x2="{r["x1"]:.1f}" y2="{r["y1"]:.1f}" stroke="#3b82f6" stroke-width="{max(0.4, r.get("width", 0.6)):.2f}"/>')
    for c in page["chars"]:
        b = c["bbox"]
        sz = max(4.0, b["y1"] - b["y0"])
        els.append(f'<text x="{b["x0"]:.1f}" y="{b["y1"] - sz * 0.13:.1f}" font-size="{sz:.1f}">{esc(c["text"])}</text>')
    return f'<svg viewBox="0 0 {w:.0f} {h:.0f}" class="page" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid meet">{"".join(els)}</svg>'


def md_html(md, n=40):
    out, tbl = [], False
    for line in md.split("\n")[:400]:
        s = line.rstrip()
        if s.startswith("|") and set(s.replace("|", "").replace("-", "").replace(" ", "")) == set():
            continue
        if s.startswith("|"):
            cells = "".join(f"<td>{esc(c.strip())}</td>" for c in s.strip("|").split("|"))
            out.append(("" if tbl else "<table>") + f"<tr>{cells}</tr>")
            tbl = True
            continue
        if tbl:
            out.append("</table>")
            tbl = False
        if s.startswith("# "): out.append(f"<h3>{esc(s[2:])}</h3>")
        elif s.startswith("## "): out.append(f"<h4>{esc(s[3:])}</h4>")
        elif s.startswith("- "): out.append(f"<li>{esc(s[2:])}</li>")
        elif s.strip(): out.append(f"<p>{esc(s)}</p>")
    if tbl:
        out.append("</table>")
    return "\n".join(out)


def render_original(path, max_pages):
    try:
        import fitz
    except ImportError:
        return []
    out = []
    doc = fitz.open(path)
    for i, pg in enumerate(doc):
        if i >= max_pages:
            break
        png = pg.get_pixmap(matrix=fitz.Matrix(1.3, 1.3)).tobytes("png")
        out.append(base64.b64encode(png).decode())
    return out


def section(binary, path, max_pages):
    name = os.path.basename(path)
    j = subprocess.run([binary, "parse", path, "--format", "json"], capture_output=True)
    if j.returncode != 0:
        return f'<section><h2>{esc(name)}</h2><p class="err">解析失败: {esc(j.stderr.decode()[:200])}</p></section>'
    doc = json.loads(j.stdout)
    md = subprocess.run([binary, "parse", path, "--format", "md"], capture_output=True).stdout.decode()
    pages = doc["pages"]
    stats = f'{len(pages)} 页 · {sum(len(p["chars"]) for p in pages):,} 字符 · {sum(len(p["rules"]) for p in pages)} 线 · {len(doc.get("warnings", []))} 警告'
    ocr_pages = {w.get("page") for w in doc.get("warnings", []) if w.get("kind") == "NeedsOcr"}
    origs = render_original(path, max_pages)
    rows = []
    for i, p in enumerate(pages[:max_pages]):
        o = f'<figure><figcaption>原始 PDF · 第{i+1}页</figcaption><img src="data:image/png;base64,{origs[i]}"/></figure>' if i < len(origs) else ""
        if not p["chars"]:
            note = "无文字层(扫描件/整页图片)· 需 OCR" if i in ocr_pages else "此页无提取到文字"
            recon = f'<div class="empty">{note}</div>'
        else:
            recon = svg(p)
        rows.append(f'<div class="cmp">{o}<figure><figcaption>pdfmuse 坐标还原</figcaption>{recon}</figure></div>')
    return f'<section><h2>{esc(name)}</h2><div class="sub">{stats}</div>{"".join(rows)}<details><summary>结构化 Markdown</summary><div class="md">{md_html(md)}</div></details></section>'


CSS = """
:root{--ink:#0f172a;--muted:#64748b;--line:#e2e8f0;--bg:#f8fafc}
*{box-sizing:border-box}body{margin:0;font-family:-apple-system,"PingFang SC","Microsoft YaHei",Segoe UI,sans-serif;color:var(--ink);background:var(--bg)}
header{padding:18px 26px;background:#fff;border-bottom:1px solid var(--line)}header h1{margin:0;font-size:17px}header p{margin:5px 0 0;color:var(--muted);font-size:12px}
section{background:#fff;border:1px solid var(--line);border-radius:14px;margin:16px 26px;padding:16px 18px}
section h2{font-size:15px;margin:0;word-break:break-all}.sub{color:var(--muted);font-size:12px;margin:4px 0 12px}.err{color:#b91c1c}
.cmp{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-bottom:12px}@media(max-width:820px){.cmp{grid-template-columns:1fr}}
figure{margin:0}figcaption{font-size:11px;color:var(--muted);margin-bottom:5px}
figure img,svg.page{width:100%;height:auto;border:1px solid var(--line);border-radius:6px;box-shadow:0 1px 4px rgba(0,0,0,.05)}
svg.page text{fill:#111;font-family:"PingFang SC","Microsoft YaHei",serif}
.empty{border:1px dashed #f59e0b;border-radius:6px;background:#fffbeb;color:#b45309;font-size:13px;display:flex;align-items:center;justify-content:center;min-height:180px;text-align:center;padding:20px}
details{margin-top:8px}summary{cursor:pointer;font-size:13px;font-weight:600;color:#334155}
.md{margin-top:10px;font-size:12.5px;line-height:1.6;max-height:360px;overflow:auto;border:1px solid var(--line);border-radius:8px;padding:12px;background:var(--bg)}
.md h3{font-size:15px}.md h4{font-size:13px}.md p{margin:.35em 0}.md table{border-collapse:collapse;margin:8px 0}.md td{border:1px solid var(--line);padding:3px 7px;font-size:12px}
"""


def main():
    ap = argparse.ArgumentParser(description="Visual fidelity check for pdfmuse.")
    ap.add_argument("files", nargs="*", help="PDF files to check")
    ap.add_argument("--random", type=int, metavar="N", help="pick N random PDFs from --dir")
    ap.add_argument("--dir", action="append", default=[], help="directory to sample (repeatable)")
    ap.add_argument("--seed", default="0", help="seed for reproducible random pick")
    ap.add_argument("--max-pages", type=int, default=3, help="pages rendered per PDF (default 3)")
    ap.add_argument("--out", help="output HTML path (default: temp file)")
    ap.add_argument("--no-open", action="store_true", help="don't open the result")
    args = ap.parse_args()

    files = [f for f in args.files if is_pdf(f)]
    if args.random:
        dirs = args.dir or [os.path.join(ROOT, "tests/corpus")]
        files += pick_random(dirs, args.random, args.seed)
    if not files:
        sys.exit("no PDFs. Pass files, or --random N --dir DIR.")

    binary = find_bin()
    sections = [section(binary, f, args.max_pages) for f in files]
    body = f'<header><h1>pdfmuse 可视化抽检 · {len(files)} 份</h1><p>左=原始 PDF,右=纯用抽取坐标还原;下=结构化 Markdown。二者对得上即证明抽取精确。</p></header>' + "".join(sections)
    doc = f'<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>pdfmuse visual check</title><style>{CSS}</style></head><body>{body}</body></html>'

    out = args.out or os.path.join(tempfile.gettempdir(), "pdfmuse_visual_check.html")
    with open(out, "w") as f:
        f.write(doc)
    print(f"wrote {out} ({len(doc)//1024} KB, {len(files)} docs)")
    for f in files:
        print("  ·", os.path.basename(f))
    if not args.no_open:
        opener = "open" if sys.platform == "darwin" else "xdg-open"
        subprocess.run([opener, out], check=False)


if __name__ == "__main__":
    main()
