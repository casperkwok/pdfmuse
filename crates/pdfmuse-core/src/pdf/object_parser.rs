//! An O(1)-offset PDF indirect-object parser producing lopdf `Object`s.
//!
//! lopdf's own parser slices a `nom_locate` span from the buffer start, making
//! every `get_object` O(offset) — the ceiling that kept large files slow. This
//! parser indexes straight to a byte offset (O(1)) and parses one object there,
//! producing the *same* lopdf `Object` values (verified byte-identical to lopdf on
//! the real-PDF corpus), so the whole downstream pipeline is unchanged.
//!
//! Semantics mirror the PDF object grammar and lopdf's choices exactly: references
//! (`N G R`) are recognized before bare numbers; reals (a dot present) parse via
//! `f32::from_str` on the token slice; literal-string end-of-line markers collapse
//! to a single `\n`. Stream bodies are read using `/Length` (resolving an indirect
//! length through the xref).

use lopdf::xref::{Xref, XrefEntry};
use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat};

pub(super) struct ObjParser<'a> {
    buf: &'a [u8],
    xref: &'a Xref,
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

impl<'a> ObjParser<'a> {
    pub(super) fn new(buf: &'a [u8], xref: &'a Xref) -> Self {
        Self { buf, xref }
    }

    /// Parse the indirect object (`N G obj … endobj`) at `offset`.
    pub(super) fn indirect_at(&self, offset: usize) -> Option<(ObjectId, Object)> {
        let mut p = offset;
        self.skip_ws(&mut p);
        let num = self.read_uint(&mut p)? as u32;
        self.skip_ws(&mut p);
        let gen = self.read_uint(&mut p)? as u16;
        self.skip_ws(&mut p);
        if !self.eat(&mut p, b"obj") {
            return None;
        }
        let obj = self.object(&mut p)?;
        Some(((num, gen), obj))
    }

    /// Resolve an object id to its offset and parse it. Used for indirect
    /// `/Length` and (by callers) on-demand resolution.
    pub(super) fn resolve(&self, id: ObjectId) -> Option<Object> {
        match self.xref.get(id.0)? {
            XrefEntry::Normal { offset, generation } if *generation == id.1 => {
                self.indirect_at(*offset as usize).map(|(_, o)| o)
            }
            _ => None,
        }
    }

    /// Parse one direct object at `*p`, advancing `*p` past it.
    fn object(&self, p: &mut usize) -> Option<Object> {
        self.skip_ws(p);
        let b = *self.buf.get(*p)?;
        match b {
            b'<' => {
                if self.buf.get(*p + 1) == Some(&b'<') {
                    self.dict_or_stream(p)
                } else {
                    Some(Object::String(self.hex_string(p), StringFormat::Hexadecimal))
                }
            }
            b'(' => Some(Object::String(self.literal_string(p), StringFormat::Literal)),
            b'/' => Some(Object::Name(self.name(p))),
            b'[' => self.array(p),
            b'0'..=b'9' | b'+' | b'-' | b'.' => Some(self.number_or_ref(p)),
            b't' if self.eat(p, b"true") => Some(Object::Boolean(true)),
            b'f' if self.eat(p, b"false") => Some(Object::Boolean(false)),
            b'n' if self.eat(p, b"null") => Some(Object::Null),
            _ => None,
        }
    }

    /// A number, or an `N G R` reference (lopdf recognizes references first).
    fn number_or_ref(&self, p: &mut usize) -> Object {
        let start = *p;
        let (int_val, is_real) = self.scan_number(p);
        if !is_real && int_val >= 0 {
            // Look ahead for "<gen> R" → indirect reference.
            let save = *p;
            self.skip_ws(p);
            if let Some(gen) = self.try_uint(p) {
                self.skip_ws(p);
                if self.eat(p, b"R") {
                    return Object::Reference((int_val as u32, gen as u16));
                }
            }
            *p = save;
        }
        let slice = &self.buf[start..*p];
        if is_real {
            let s = std::str::from_utf8(slice).unwrap_or("0");
            Object::Real(s.parse::<f32>().unwrap_or(0.0))
        } else {
            Object::Integer(int_val)
        }
    }

    /// Scan a numeric token, returning (integer value if not real, is_real).
    fn scan_number(&self, p: &mut usize) -> (i64, bool) {
        let start = *p;
        if matches!(self.buf.get(*p), Some(b'+') | Some(b'-')) {
            *p += 1;
        }
        let mut is_real = false;
        while let Some(&c) = self.buf.get(*p) {
            if c.is_ascii_digit() {
                *p += 1;
            } else if c == b'.' {
                is_real = true;
                *p += 1;
            } else {
                break;
            }
        }
        let int_val = if is_real {
            0
        } else {
            std::str::from_utf8(&self.buf[start..*p]).ok().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0)
        };
        (int_val, is_real)
    }

    fn dict_or_stream(&self, p: &mut usize) -> Option<Object> {
        let dict = self.dict(p)?;
        // A dictionary immediately followed by `stream` is a stream object.
        let save = *p;
        self.skip_ws(p);
        if self.eat(p, b"stream") {
            // One CRLF or LF follows the `stream` keyword.
            if self.buf.get(*p) == Some(&b'\r') {
                *p += 1;
            }
            if self.buf.get(*p) == Some(&b'\n') {
                *p += 1;
            }
            let body_start = *p;
            let len = self.stream_length(&dict);
            let content = match len {
                Some(n) if body_start + n <= self.buf.len() => self.buf[body_start..body_start + n].to_vec(),
                _ => self.stream_body_fallback(body_start),
            };
            return Some(Object::Stream(Stream::new(dict, content)));
        }
        *p = save;
        Some(Object::Dictionary(dict))
    }

    /// `/Length` from the stream dict, resolving an indirect reference via the xref.
    fn stream_length(&self, dict: &Dictionary) -> Option<usize> {
        match dict.get(b"Length").ok()? {
            Object::Integer(n) => usize::try_from(*n).ok(),
            Object::Reference(id) => match self.resolve(*id)? {
                Object::Integer(n) => usize::try_from(n).ok(),
                _ => None,
            },
            _ => None,
        }
    }

    /// When `/Length` is missing or wrong, scan to `endstream` (minus its EOL).
    fn stream_body_fallback(&self, start: usize) -> Vec<u8> {
        let mut end = start;
        while end + 9 <= self.buf.len() {
            if &self.buf[end..end + 9] == b"endstream" {
                let mut e = end;
                if e > start && self.buf[e - 1] == b'\n' {
                    e -= 1;
                }
                if e > start && self.buf[e - 1] == b'\r' {
                    e -= 1;
                }
                return self.buf[start..e].to_vec();
            }
            end += 1;
        }
        self.buf[start..].to_vec()
    }

    fn dict(&self, p: &mut usize) -> Option<Dictionary> {
        *p += 2; // consume "<<"
        let mut dict = Dictionary::new();
        loop {
            self.skip_ws(p);
            match self.buf.get(*p) {
                Some(b'>') if self.buf.get(*p + 1) == Some(&b'>') => {
                    *p += 2;
                    return Some(dict);
                }
                Some(b'/') => {
                    let key = self.name(p);
                    let value = self.object(p)?;
                    dict.set(key, value);
                }
                _ => return None,
            }
        }
    }

    fn array(&self, p: &mut usize) -> Option<Object> {
        *p += 1; // consume "["
        let mut items = Vec::new();
        loop {
            self.skip_ws(p);
            match self.buf.get(*p) {
                Some(b']') => {
                    *p += 1;
                    return Some(Object::Array(items));
                }
                Some(_) => items.push(self.object(p)?),
                None => return None,
            }
        }
    }

    fn name(&self, p: &mut usize) -> Vec<u8> {
        *p += 1; // consume "/"
        let mut out = Vec::new();
        while let Some(&b) = self.buf.get(*p) {
            if is_ws(b) || is_delim(b) {
                break;
            }
            *p += 1;
            if b == b'#' {
                let h = self.buf.get(*p).copied().and_then(hex_val);
                let l = self.buf.get(*p + 1).copied().and_then(hex_val);
                if let (Some(h), Some(l)) = (h, l) {
                    out.push((h << 4) | l);
                    *p += 2;
                    continue;
                }
            }
            out.push(b);
        }
        out
    }

    fn literal_string(&self, p: &mut usize) -> Vec<u8> {
        *p += 1; // consume "("
        let mut out = Vec::new();
        let mut depth = 1;
        while let Some(&b) = self.buf.get(*p) {
            *p += 1;
            match b {
                b'\\' => {
                    let Some(&e) = self.buf.get(*p) else { break };
                    *p += 1;
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
                            if self.buf.get(*p) == Some(&b'\n') {
                                *p += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut val = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.buf.get(*p) {
                                    Some(&d @ b'0'..=b'7') => {
                                        val = val * 8 + (d - b'0') as u32;
                                        *p += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(val as u8);
                        }
                        other => out.push(other),
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
                // A raw end-of-line inside a literal string is a single LF.
                b'\r' => {
                    if self.buf.get(*p) == Some(&b'\n') {
                        *p += 1;
                    }
                    out.push(b'\n');
                }
                _ => out.push(b),
            }
        }
        out
    }

    fn hex_string(&self, p: &mut usize) -> Vec<u8> {
        *p += 1; // consume "<"
        let mut out = Vec::new();
        let mut hi: Option<u8> = None;
        while let Some(&b) = self.buf.get(*p) {
            *p += 1;
            if b == b'>' {
                break;
            }
            if let Some(v) = hex_val(b) {
                match hi.take() {
                    None => hi = Some(v),
                    Some(h) => out.push((h << 4) | v),
                }
            }
        }
        if let Some(h) = hi {
            out.push(h << 4);
        }
        out
    }

    // --- low-level helpers ---

    fn skip_ws(&self, p: &mut usize) {
        while let Some(&b) = self.buf.get(*p) {
            if is_ws(b) {
                *p += 1;
            } else if b == b'%' {
                while let Some(&c) = self.buf.get(*p) {
                    *p += 1;
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn eat(&self, p: &mut usize, kw: &[u8]) -> bool {
        if self.buf.get(*p..*p + kw.len()) == Some(kw) {
            *p += kw.len();
            true
        } else {
            false
        }
    }

    fn read_uint(&self, p: &mut usize) -> Option<u64> {
        self.try_uint(p)
    }

    fn try_uint(&self, p: &mut usize) -> Option<u64> {
        let start = *p;
        while matches!(self.buf.get(*p), Some(c) if c.is_ascii_digit()) {
            *p += 1;
        }
        if *p == start {
            return None;
        }
        std::str::from_utf8(&self.buf[start..*p]).ok()?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::xref::XrefType;

    fn parse(bytes: &[u8]) -> Object {
        let xref = Xref::new(0, XrefType::CrossReferenceTable);
        ObjParser::new(bytes, &xref).indirect_at(0).unwrap().1
    }

    #[test]
    fn parses_dict_with_types_and_refs() {
        let obj = parse(b"7 0 obj\n<< /Type /Page /Count 3 /W 1.5 /P 4 0 R /N (hi) >>\nendobj");
        let Object::Dictionary(d) = obj else { panic!("expected dict") };
        assert_eq!(d.get(b"Type").unwrap().as_name().unwrap(), b"Page");
        assert_eq!(d.get(b"Count").unwrap().as_i64().unwrap(), 3); // integer, not real
        assert!((d.get(b"W").unwrap().as_f32().unwrap() - 1.5).abs() < 1e-6); // real
        assert_eq!(d.get(b"P").unwrap().as_reference().unwrap(), (4, 0)); // ref, not "4 0"
        assert!(matches!(d.get(b"N").unwrap(), Object::String(s, _) if s == b"hi"));
    }

    #[test]
    fn integer_not_confused_with_reference() {
        // "3" followed by a name (not "G R") stays an integer.
        let obj = parse(b"1 0 obj\n[ 3 4 5 ]\nendobj");
        let Object::Array(a) = obj else { panic!() };
        assert_eq!(a.len(), 3);
        assert_eq!(a[0].as_i64().unwrap(), 3);
    }

    #[test]
    fn stream_body_read_by_length() {
        let obj = parse(b"2 0 obj\n<< /Length 5 >>\nstream\nhello\nendstream\nendobj");
        let Object::Stream(s) = obj else { panic!("expected stream") };
        assert_eq!(s.content, b"hello");
    }

    #[test]
    fn hex_string_and_escaped_name() {
        let obj = parse(b"1 0 obj\n<< /K <48656C6C6F> /N /A#42 >>\nendobj");
        let Object::Dictionary(d) = obj else { panic!() };
        assert!(matches!(d.get(b"K").unwrap(), Object::String(s, _) if s == b"Hello"));
        assert_eq!(d.get(b"N").unwrap().as_name().unwrap(), b"AB"); // #42 = 'B'
    }
}
