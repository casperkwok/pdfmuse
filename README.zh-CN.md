<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/pdfmuse-logo-dark.svg">
    <img alt="pdfmuse" src="assets/pdfmuse-logo.svg" width="340">
  </picture>
</p>

<p align="center"><a href="README.md">English</a> · <strong>中文</strong></p>

<p align="center">
  <a href="https://crates.io/crates/pdfmuse-core"><img alt="crates.io" src="https://img.shields.io/crates/v/pdfmuse-core?logo=rust&logoColor=white&label=crates.io&color=E43716"></a>
  <a href="https://pypi.org/project/pdfmuse/"><img alt="PyPI" src="https://img.shields.io/pypi/v/pdfmuse?logo=pypi&logoColor=white&label=PyPI&color=3775A9"></a>
  <a href="https://www.npmjs.com/package/@pdfmuse/node"><img alt="npm" src="https://img.shields.io/npm/v/%40pdfmuse%2Fnode?logo=npm&label=npm&color=CB3837"></a>
  <a href="https://github.com/casperkwok/pdfmuse/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/casperkwok/pdfmuse/ci.yml?branch=main&logo=github&label=CI"></a>
  <a href="https://casperkwok.github.io/pdfmuse/"><img alt="live demo" src="https://img.shields.io/badge/demo-live-6E56CF?logo=webassembly&logoColor=white"></a>
  <img alt="license" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue">
</p>

<p align="center">
  <a href="https://casperkwok.github.io/pdfmuse/"><strong>▶ 在线体验</strong></a> —— 拖一个 PDF,在浏览器里实时解析(文件不上传)
</p>

<p align="center">
  <a href="https://casperkwok.github.io/pdfmuse/"><img src="assets/pdfmuse.gif" alt="pdfmuse playground:原图 ↔ pdfmuse 坐标还原" width="760"></a>
</p>

**面向 RAG / LLM 的确定性 PDF/DOCX 解析器** —— 单一 Rust 核心,配 Python、Node、WASM 三端绑定,输出**逐字节一致**。

pdfmuse 是给 AI/RAG 的**精确前置层**:把文件里真正含有的东西都抽出来——带精确坐标的文字、字体、矢量线、表格、链接——又快、又稳、且**每个绑定输出完全一致**。它在 ML 边界干净收手:OCR 与视觉版面推断交给可插拔后端,核心保持确定性、**零 ML 依赖**。它**不是**又一个概率性视觉模型。

## 为什么用 pdfmuse

| | |
|---|---|
| **全** | 保留最细粒度的字符 + 坐标,绝不替你悄悄丢内容。 |
| **快** | 零拷贝流式 Rust 核心,自研 O(1) 对象解析器 + 内容流分词器 + 按页并行。 |
| **稳** | 单页/对象损坏不会拖垮整篇——返回结构化错误,永不 panic(经 fuzz 验证)。 |
| **确定** | 相同输入 → 相同输出。核心路径无概率模型、无时间/随机。 |
| **一致** | Python / Node / WASM 调用同一 Rust 核心,输出**逐字节一致**(CI 强制)。 |
| **中文一等公民** | CID/Type0 字体 + CMap/ToUnicode 走主路径;兼容区码点 NFKC 归一化,检索干净。 |

## 性能

给 RAG 前置层看两件事:多快、内容丢不丢。两者都在**公开、可复现的语料**上测——61 篇 arXiv 论文、横跨 8 个学科(大而密的 PDF,是刻意挑的难例),你可以自己重跑完全一样的 benchmark:

```bash
python benches/fetch_corpus.py --out /tmp/corpus      # 固定清单,下同一批 PDF
pip install "pdfmuse==0.1.8" "pymupdf==1.28.0" "pdfplumber==0.11.10"
python benches/compare.py --dir /tmp/corpus
```

**文本抽取**(`to_text`,warm-up 后取 7 次中位数;PyMuPDF 1.28 / MuPDF 1.29、pdfplumber 0.11、macOS arm64):

| 对比 | 加速(几何均值) | 胜率 | 最差情况 |
|---|---|---|---|
| **PyMuPDF** | **约 5.9× 快** | 56 / 61(92%) | 反而慢约 3×(某 3MB 论文) |
| **pdfplumber** | **约 110× 快** | 61 / 61(100%) | 10.6× |

**诚实说明**:pdfmuse **并非永远最快**。在最大最密的 5 篇(2–22MB)上,PyMuPDF 成熟的 C 核(MuPDF)更快——最多快约 3×。其余 92% pdfmuse 赢;在典型 RAG 文档(简历/报告/发票,1–2 页)上约 1–2ms、稳定赢。**内容不丢**:相对 PyMuPDF 非空白字符中位覆盖率 **100%**(n=61)。

`to_text()` / `to_markdown()` 直接从 Rust 核心返回字符串(不反序列化整棵 IR)。完整 `parse()`(字符+bbox+表格,远多于纯文本)只比 `to_text` 慢约 2.3×,多数文件仍快过 PyMuPDF。原生 Node 绑定 ≈ Rust 核心速度,WASM 约 1.7×。用 `examples/visual_check.py` 肉眼验保真度。

## 安装

```bash
# Rust
cargo add pdfmuse-core
# Python(abi3 wheels)
pip install pdfmuse
# Node（原生绑定）
npm install @pdfmuse/node
# WASM（浏览器）
npm install @pdfmuse/core   # 或自行构建：wasm-pack build crates/pdfmuse-wasm --target web
```

## 用法

**CLI**（调试/查看）:
```bash
pdfmuse parse report.pdf --format md      # 结构化 Markdown（标题、表格）
pdfmuse parse report.pdf --format json    # 完整 IR（字符、bbox、块、警告）
```

**Rust**:
```rust
let data = std::fs::read("report.pdf")?;
let doc = pdfmuse_core::parse(&data, None)?;                 // 自动识别 PDF/DOCX
for page in &doc.pages {
    for ch in &page.chars { /* ch.text, ch.bbox {x0,y0,x1,y1}, ch.size */ }
}
let md = pdfmuse_core::to_markdown(&doc);
let chunks = pdfmuse_core::chunk(&doc);                      // RAG 分块 + {page, bbox, heading_path}
```

**Python**:
```python
import pdfmuse
data = open("report.pdf", "rb").read()
text = pdfmuse.to_text(data)         # 纯文本——快路径(~1.3ms,免整棵 IR 的 json.loads)
md = pdfmuse.to_markdown(data)       # 结构化 Markdown——标题(PDF & DOCX)+ 表格
doc = pdfmuse.parse(data)            # 完整 IR:doc.pages[i].chars/blocks 带 bbox
clean = pdfmuse.to_text(data, drop_boilerplate=True)  # 去掉跨页页眉/页脚
```

**Node**:
```js
const { toText, toMarkdown, parse } = require("@pdfmuse/node");
const data = fs.readFileSync("report.pdf");
const text = toText(data);           // 纯文本——快路径
const clean = toText(data, undefined, true);  // 去掉跨页页眉/页脚
const doc = parse(data);             // 完整 IR(带类型的 Document)
```

**WASM**（浏览器——数字版 PDF;扫描页返回 `NeedsOcr` 警告,交由服务端处理）:
```js
import init, { to_text, parse } from "@pdfmuse/core";
await init();
const text = to_text(new Uint8Array(bytes));          // 纯文本
const doc = JSON.parse(parse(new Uint8Array(bytes))); // 完整 IR
```

## 生态集成

- **LangChain** —— [`langchain-pdfmuse`](integrations/langchain-pdfmuse):`PdfmuseLoader`,支持 `single` / `page` / `elements` 模式。`elements` 模式下每个 chunk 带**分节元数据**(`heading_path`、`bbox`、`category`)—— 给 RAG 的可复现分块。

  ```python
  from langchain_pdfmuse import PdfmuseLoader
  docs = PdfmuseLoader("report.pdf", mode="elements").load()
  ```

- **LlamaIndex** —— [`llama-index-readers-pdfmuse`](integrations/llama-index-pdfmuse):`PdfmuseReader`,同样的模式与分节元数据。

  ```python
  from llama_index.readers.pdfmuse import PdfmuseReader
  docs = PdfmuseReader(mode="elements").load_data("report.pdf")
  ```

- **Haystack** —— [`pdfmuse-haystack`](integrations/haystack-pdfmuse):`PdfmuseConverter` 组件(`text` / `markdown`),用于 Haystack 2.x pipeline。

  ```python
  from pdfmuse_haystack import PdfmuseConverter
  docs = PdfmuseConverter(mode="markdown").run(sources=["report.pdf"])["documents"]
  ```

## 能力边界

**在核心内（确定性）**:文字 + 坐标/字体/字号/颜色 · 矢量线与矩形 · 行/段/分栏聚类 · 标题识别(字号 + 编号) · 跨页页眉页脚识别 + 可选去除 · 有线表格与空白对齐表格重建 · 完整 DOCX 结构 · JSON / Markdown / RAG 分块输出。

**在核心外（可插拔 `VisionBackend`）**:扫描件 OCR · 无框表格结构识别 · 标题/正文/图注分类。无文字层（扫描/图片）的页会标 `NeedsOcr`,交给后端——见 [`docs/adr/0001-pdf-engine-strategy.md`](docs/adr/0001-pdf-engine-strategy.md)。

守住这条边界,正是 pdfmuse 又快又稳、区别于视觉模型的原因。

## 目录结构

```
crates/
  pdfmuse-core/     纯 Rust 核心：PDF/DOCX → 统一 IR（解析器、分词器、版面、输出）
  pdfmuse-python/   PyO3（abi3）绑定
  pdfmuse-node/     napi-rs 绑定
  pdfmuse-wasm/     wasm-bindgen 绑定
  pdfmuse-cli/      调试 CLI（pdfmuse）
tests/{corpus,snapshots}   金标语料 + insta 快照
tests/parity/              三端逐字节一致门禁（Python == Node == WASM）
examples/visual_check.py   原图 ↔ 坐标还原可视化抽检
fuzz/                      cargo-fuzz 目标（永不 panic）
```

## 测试门禁

- **快照测试**（`insta` + `tests/corpus`）
- **三端一致性 CI** —— Python/Node/WASM 逐字节一致(红则阻断合并)
- **鲁棒性** —— 畸形/随机输入永不 panic（`tests/robustness.rs` + `fuzz/`）
- **中文正确性**专项

## 状态

核心功能完整(里程碑 M0–M4 + 真机加固 M4.5):PDF + DOCX → 统一 IR → JSON / Markdown / RAG 分块,三端逐字节一致,加密,中文。当前处于 **M5 · 打磨与发布**。路线图与任务在 Linear(项目 **pdfmuse**)。

## 许可

双许可:[MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE),任选其一。
