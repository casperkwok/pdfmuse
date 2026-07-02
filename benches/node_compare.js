// Compare pdfmuse (Node binding) vs pdf-parse (the common JS extractor, pdf.js-based).
const fs = require("fs");
const path = require("path");
const native = require("../bindings/node/native.js");
const { PDFParse } = require("pdf-parse"); // pdf.js-based, common JS extractor

function listPdfs(dir) {
  let out = [];
  try {
    for (const f of fs.readdirSync(dir)) {
      if (!f.toLowerCase().endsWith(".pdf")) continue;
      const p = path.join(dir, f);
      const fd = fs.openSync(p, "r");
      const buf = Buffer.alloc(5);
      fs.readSync(fd, buf, 0, 5, 0);
      fs.closeSync(fd);
      if (buf.toString() === "%PDF-") out.push(p);
    }
  } catch (e) {}
  return out.sort();
}

const files = [
  ...listPdfs("/Users/peguo/Developer/Projects/MyResume"),
  ...listPdfs("/Users/peguo/Downloads"),
].slice(0, 22);

const RUNS = 5;
const nws = (s) => s.replace(/\s/g, "").length;
const median = (a) => a.sort((x, y) => x - y)[Math.floor(a.length / 2)];

async function benchAsync(fn, buf) {
  let ts = [], out = "";
  for (let i = 0; i < RUNS; i++) {
    const t0 = process.hrtime.bigint();
    out = await fn(buf);
    ts.push(Number(process.hrtime.bigint() - t0) / 1e6);
  }
  return [median(ts), out];
}

(async () => {
  const ratios = [];
  console.log(`${"file".padEnd(34)} ${"pdfmuse".padStart(9)} ${"pdf-parse".padStart(10)}  ${"muse_ch".padStart(7)} ${"pp_ch".padStart(7)}`);
  console.log("-".repeat(78));
  for (const p of files) {
    const buf = fs.readFileSync(p);
    let museT, museTxt, ppT, ppTxt;
    try {
      [museT, museTxt] = await benchAsync((b) => native.parse_buffer(b), buf);
      const doc = JSON.parse(museTxt);
      museTxt = doc.pages.map((pg) => pg.chars.map((c) => c.text).join("")).join("\n");
    } catch (e) { museT = NaN; museTxt = ""; }
    try {
      [ppT, ppTxt] = await benchAsync(async (b) => (await new PDFParse({ data: b }).getText()).text, buf);
    } catch (e) { ppT = NaN; ppTxt = "ERR"; }
    if (museT === museT && ppT === ppT) ratios.push(ppT / museT);
    console.log(`${path.basename(p).slice(0, 33).padEnd(34)} ${museT.toFixed(1).padStart(9)} ${ppT.toFixed(1).padStart(10)}  ${String(nws(museTxt)).padStart(7)} ${String(nws(ppTxt)).padStart(7)}`);
  }
  const clean = ratios.filter((r) => r === r && r > 0);
  const gm = Math.exp(clean.reduce((a, b) => a + Math.log(b), 0) / clean.length);
  console.log("-".repeat(78));
  console.log(`\nfiles: ${files.length}  runs/file: ${RUNS}`);
  console.log(`speed: pdfmuse is ${gm.toFixed(1)}x pdf-parse (geomean, >1 = pdfmuse faster)`);
  console.log(`pdf-parse faster on: ${clean.filter((r) => r < 1).length}/${clean.length} files`);
})();
