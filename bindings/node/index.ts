/**
 * pdfmuse — deterministic PDF/DOCX parser for RAG / LLMs.
 *
 * Thin typed wrapper over the Rust core. The native addon returns the IR as
 * JSON; here we deserialize it into typed objects. Typing deepens as the IR
 * grows — for now nested structures (chars, blocks, …) are left loosely typed.
 */

// `native.js` is the napi-generated loader (it picks the right `*.node` for the
// host platform) and re-exports the native `parse_buffer`.
import { parse_buffer } from "./native";

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
