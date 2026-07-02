/**
 * pdfmuse — deterministic PDF/DOCX parser for RAG / LLMs.
 *
 * Thin typed wrapper over the Rust core. The native addon returns the IR as
 * JSON; here we deserialize it into typed objects. Typing deepens as the IR
 * grows — for now nested structures (chars, blocks, …) are left loosely typed.
 */

// `native.js` is the napi-generated loader (it picks the right `*.node` for the
// host platform) and re-exports the native `parse_buffer`.
import { parse_buffer, text_buffer, markdown_buffer } from "./native";

export interface Page {
  index: number;
  width: number;
  height: number;
  rotation: number;
  chars: unknown[];
  lines: unknown[];
  blocks: unknown[];
  rects: unknown[];
  rules: unknown[];
  images: unknown[];
  links: unknown[];
}

export interface Document {
  source: string;
  metadata: Record<string, unknown>;
  pages: Page[];
  outline: unknown[];
  warnings: unknown[];
}

/**
 * Parse PDF/DOCX bytes into a {@link Document}.
 *
 * `fmt` forces a format (`"pdf"`/`"docx"`); omit it to auto-detect from magic
 * bytes.
 */
export function parse(data: Buffer, fmt?: "pdf" | "docx"): Document {
  return JSON.parse(parse_buffer(data, fmt)) as Document;
}

/**
 * Parse and return plain reading-order text. Faster than {@link parse} when you
 * only need text: the Rust core returns one string, so there is no full-IR JSON
 * to `JSON.parse` on the JS side.
 */
export function toText(data: Buffer, fmt?: "pdf" | "docx"): string {
  return text_buffer(data, fmt);
}

/** Parse and return structured Markdown (headings + tables) as one string. */
export function toMarkdown(data: Buffer, fmt?: "pdf" | "docx"): string {
  return markdown_buffer(data, fmt);
}
