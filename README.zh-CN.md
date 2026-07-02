<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/pdfmuse-logo-dark.svg">
    <img alt="pdfmuse" src="assets/pdfmuse-logo.svg" width="340">
  </picture>
</p>

<p align="center"><a href="README.md">English</a> · <strong>中文</strong></p>

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

在 22 个真实 PDF(简历、报告、发票;7 次中位数,core-to-core,各自返回字符串)上实测:

| 对比 | 结果 |
|---|---|
| **pdfplumber**(Python,RAG 常用) | **快约 28–39×** |
| **PyMuPDF**(成熟 C 库) | **快约 4×**(样本内每个文件都赢) |

文字内容不丢(相对 PyMuPDF 非空白字符中位覆盖率 100%)。见 [`benches/`](benches) 与 `examples/visual_check.py`。数字随硬件而异——请本地复现。

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
doc = pdfmuse.parse(open("report.pdf", "rb").read())
text = "".join(c.text for pg in doc.pages for c in pg.chars)
```

**Node**:
```js
const { parse_buffer } = require("@pdfmuse/node");
const doc = JSON.parse(parse_buffer(fs.readFileSync("report.pdf")));
```

**WASM**（浏览器——数字版 PDF;扫描页返回 `NeedsOcr` 警告,交由服务端处理）:
```js
import init, { parse } from "@pdfmuse/core";
await init();
const doc = JSON.parse(parse(new Uint8Array(bytes)));
```

## 能力边界

**在核心内（确定性）**:文字 + 坐标/字体/字号/颜色 · 矢量线与矩形 · 行/段/分栏聚类 · 有线表格与空白对齐表格重建 · 完整 DOCX 结构 · JSON / Markdown / RAG 分块输出。

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
