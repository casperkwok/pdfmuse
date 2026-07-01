//! Zero-allocation content-stream tokenizer.
//!
//! Replaces lopdf's `Content::decode`, which allocates a `String` operator and a
//! `Vec<Object>` of operands **per operation** — dominating parse time on
//! graphics-heavy pages (a resume with 113k path ops spent 65ms just tokenizing).
//! This lexer scans the decompressed bytes once, pushing operands onto a reusable
//! stack; the interpreter dispatches on the operator as a `&[u8]` slice and clears
//! the stack. Numbers cost no allocation at all (the common case in vector art).
//!
//! Semantics mirror the PDF spec's object grammar exactly so output stays
//! byte-identical to the previous lopdf path (enforced by the parity gate and a
//! full-corpus regression diff).

/// An operand the interpreter consumes. Only the kinds our interpreter reads are
/// modeled; booleans/null and dicts (marked-content / inline-image params) are
/// skipped by the lexer since no operator we handle takes them.
pub(super) enum Operand {
    Num(f32),
    Str(Vec<u8>),
    Name(Vec<u8>),
    Array(Vec<Operand>),
}

pub(super) enum Token<'a> {
    Operand(Operand),
    /// An operator keyword, borrowed from the content bytes (no allocation).
    Operator(&'a [u8]),
}

pub(super) struct Lexer<'a> {
    data: &'a [u8],
    pos: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b'\0' | b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

impl<'a> Lexer<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Advance the position past a `BI ... ID <binary> EI` inline image (called by
    /// the interpreter when it sees the `BI` operator). The binary payload must not
    /// be tokenized. Per spec `EI` is delimited by whitespace on both sides.
    pub(super) fn skip_inline_image(&mut self) {
        // Skip the parameter dict up to the `ID` operator.
        while self.pos < self.data.len() {
            if self.data[self.pos] == b'I' && self.data.get(self.pos + 1) == Some(&b'D') {
                self.pos += 2;
                break;
            }
            self.pos += 1;
        }
        // Exactly one whitespace byte follows `ID` before the raw data.
        if self.pos < self.data.len() && is_ws(self.data[self.pos]) {
            self.pos += 1;
        }
        // Scan for whitespace-delimited `EI`.
        while self.pos + 1 < self.data.len() {
            let ws_before = self.pos == 0 || is_ws(self.data[self.pos - 1]);
            if ws_before && self.data[self.pos] == b'E' && self.data[self.pos + 1] == b'I' {
                let after = self.data.get(self.pos + 2).copied();
                if after.is_none_or(|c| is_ws(c) || is_delim(c)) {
                    self.pos += 2;
                    return;
                }
            }
            self.pos += 1;
        }
        self.pos = self.data.len();
    }

    fn skip_ws_and_comments(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if is_ws(b) {
                self.pos += 1;
            } else if b == b'%' {
                while let Some(&c) = self.data.get(self.pos) {
                    self.pos += 1;
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    pub(super) fn next(&mut self) -> Option<Token<'a>> {
        loop {
            self.skip_ws_and_comments();
            let b = *self.data.get(self.pos)?;
            match b {
                b'0'..=b'9' | b'+' | b'-' | b'.' => {
                    return Some(Token::Operand(Operand::Num(self.read_number())));
                }
                b'(' => return Some(Token::Operand(Operand::Str(self.read_literal_string()))),
                b'<' => {
                    if self.data.get(self.pos + 1) == Some(&b'<') {
                        self.skip_dict();
                        continue; // dict operands are never consumed by our operators
                    }
                    return Some(Token::Operand(Operand::Str(self.read_hex_string())));
                }
                b'/' => return Some(Token::Operand(Operand::Name(self.read_name()))),
                b'[' => return Some(Token::Operand(Operand::Array(self.read_array()))),
                b']' | b'>' | b'}' | b'{' | b')' => {
                    self.pos += 1; // stray delimiter — ignore defensively
                    continue;
                }
                _ => {
                    let start = self.pos;
                    while let Some(&c) = self.data.get(self.pos) {
                        if is_ws(c) || is_delim(c) {
                            break;
                        }
                        self.pos += 1;
                    }
                    if self.pos == start {
                        self.pos += 1;
                        continue;
                    }
                    let kw = &self.data[start..self.pos];
                    // Booleans/null appear only as operands to operators we ignore;
                    // dropping them can't affect the operators we handle.
                    if kw == b"true" || kw == b"false" || kw == b"null" {
                        continue;
                    }
                    return Some(Token::Operator(kw));
                }
            }
        }
    }

    fn read_number(&mut self) -> f32 {
        let start = self.pos;
        if matches!(self.data.get(self.pos), Some(b'+') | Some(b'-')) {
            self.pos += 1;
        }
        while let Some(&c) = self.data.get(self.pos) {
            if c.is_ascii_digit() || c == b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        parse_number(&self.data[start..self.pos])
    }

    fn read_literal_string(&mut self) -> Vec<u8> {
        self.pos += 1; // consume '('
        let mut out = Vec::new();
        let mut depth = 1;
        while let Some(&b) = self.data.get(self.pos) {
            self.pos += 1;
            match b {
                b'\\' => {
                    let Some(&e) = self.data.get(self.pos) else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'(' => out.push(b'('),
                        b')' => out.push(b')'),
                        b'\\' => out.push(b'\\'),
                        b'\r' => {
                            // Line continuation: an escaped CR (optionally CRLF) emits nothing.
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {} // line continuation
                        b'0'..=b'7' => {
                            // Up to three octal digits (the first already consumed).
                            let mut val = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.data.get(self.pos) {
                                    Some(&d @ b'0'..=b'7') => {
                                        val = val * 8 + (d - b'0') as u32;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(val as u8);
                        }
                        other => out.push(other), // unknown escape: drop the backslash
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(b'(');
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(b')');
                }
                _ => out.push(b),
            }
        }
        out
    }

    fn read_hex_string(&mut self) -> Vec<u8> {
        self.pos += 1; // consume '<'
        let mut out = Vec::new();
        let mut hi: Option<u8> = None;
        while let Some(&b) = self.data.get(self.pos) {
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if let Some(v) = hex_val(b) {
                match hi.take() {
                    None => hi = Some(v),
                    Some(h) => out.push((h << 4) | v),
                }
            }
            // non-hex (whitespace) is ignored per spec
        }
        if let Some(h) = hi {
            out.push(h << 4); // odd digit → low nibble is 0
        }
        out
    }

    fn read_name(&mut self) -> Vec<u8> {
        self.pos += 1; // consume '/'
        let mut out = Vec::new();
        while let Some(&b) = self.data.get(self.pos) {
            if is_ws(b) || is_delim(b) {
                break;
            }
            self.pos += 1;
            if b == b'#' {
                let h = self.data.get(self.pos).copied().and_then(hex_val);
                let l = self.data.get(self.pos + 1).copied().and_then(hex_val);
                if let (Some(h), Some(l)) = (h, l) {
                    out.push((h << 4) | l);
                    self.pos += 2;
                    continue;
                }
            }
            out.push(b);
        }
        out
    }

    fn read_array(&mut self) -> Vec<Operand> {
        self.pos += 1; // consume '['
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            let Some(&b) = self.data.get(self.pos) else { break };
            match b {
                b']' => {
                    self.pos += 1;
                    break;
                }
                b'0'..=b'9' | b'+' | b'-' | b'.' => items.push(Operand::Num(self.read_number())),
                b'(' => items.push(Operand::Str(self.read_literal_string())),
                b'<' => items.push(Operand::Str(self.read_hex_string())),
                b'/' => items.push(Operand::Name(self.read_name())),
                _ => self.pos += 1, // skip anything unexpected
            }
        }
        items
    }

    fn skip_dict(&mut self) {
        self.pos += 2; // consume '<<'
        let mut depth = 1;
        while self.pos < self.data.len() && depth > 0 {
            match self.data[self.pos] {
                b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                    depth += 1;
                    self.pos += 2;
                }
                b'>' if self.data.get(self.pos + 1) == Some(&b'>') => {
                    depth -= 1;
                    self.pos += 2;
                }
                b'(' => {
                    let _ = self.read_literal_string(); // strings may contain >> etc.
                }
                _ => self.pos += 1,
            }
        }
    }
}

/// Parse a PDF number token into `f32`. Uses `str::parse` (correctly rounded, so
/// byte-identical to the previous lopdf path) with a hand-rolled fallback for the
/// trailing-dot / lone-sign forms it rejects.
fn parse_number(bytes: &[u8]) -> f32 {
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return 0.0,
    };
    if let Ok(v) = s.parse::<f32>() {
        return v;
    }
    let b = s.as_bytes();
    let mut i = 0;
    let sign = match b.first() {
        Some(b'-') => {
            i = 1;
            -1.0
        }
        Some(b'+') => {
            i = 1;
            1.0
        }
        _ => 1.0,
    };
    let mut int_part = 0.0f32;
    while i < b.len() && b[i].is_ascii_digit() {
        int_part = int_part * 10.0 + (b[i] - b'0') as f32;
        i += 1;
    }
    let mut frac = 0.0f32;
    let mut scale = 1.0f32;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            scale *= 10.0;
            frac = frac * 10.0 + (b[i] - b'0') as f32;
            i += 1;
        }
    }
    sign * (int_part + frac / scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nums(data: &[u8]) -> Vec<f32> {
        let mut lex = Lexer::new(data);
        let mut out = Vec::new();
        while let Some(t) = lex.next() {
            if let Token::Operand(Operand::Num(n)) = t {
                out.push(n);
            }
        }
        out
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(nums(b"1 -2 3.5 -.5 4. +7 0"), vec![1.0, -2.0, 3.5, -0.5, 4.0, 7.0, 0.0]);
    }

    #[test]
    fn tokenizes_operators_and_operands() {
        let mut lex = Lexer::new(b"1 0 0 1 72 720 cm /F1 12 Tf");
        let mut ops = Vec::new();
        while let Some(t) = lex.next() {
            if let Token::Operator(k) = t {
                ops.push(String::from_utf8_lossy(k).to_string());
            }
        }
        assert_eq!(ops, vec!["cm", "Tf"]);
    }

    #[test]
    fn literal_string_escapes() {
        let mut lex = Lexer::new(b"(a\\(b\\)\\101\\n) Tj");
        let Some(Token::Operand(Operand::Str(s))) = lex.next() else { panic!() };
        assert_eq!(s, b"a(b)A\n"); // \101 octal = 'A'
    }

    #[test]
    fn hex_string_and_name() {
        let mut lex = Lexer::new(b"<48656C6C6F> /Na#20me");
        let Some(Token::Operand(Operand::Str(s))) = lex.next() else { panic!() };
        assert_eq!(s, b"Hello");
        let Some(Token::Operand(Operand::Name(n))) = lex.next() else { panic!() };
        assert_eq!(n, b"Na me"); // #20 = space
    }

    #[test]
    fn tj_array_mixes_strings_and_numbers() {
        let mut lex = Lexer::new(b"[(A) -120 (B)] TJ");
        let Some(Token::Operand(Operand::Array(items))) = lex.next() else { panic!() };
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0], Operand::Str(s) if s == b"A"));
        assert!(matches!(items[1], Operand::Num(n) if (n - -120.0).abs() < 1e-6));
    }

    #[test]
    fn skips_dicts_and_comments() {
        let mut lex = Lexer::new(b"<</A 1>> BDC % comment\n42 Tj");
        // dict skipped; BDC operator; 42 operand; Tj operator
        let mut kinds = Vec::new();
        while let Some(t) = lex.next() {
            kinds.push(match t {
                Token::Operator(k) => String::from_utf8_lossy(k).to_string(),
                Token::Operand(Operand::Num(n)) => format!("num:{n}"),
                _ => "other".into(),
            });
        }
        assert_eq!(kinds, vec!["BDC", "num:42", "Tj"]);
    }
}
