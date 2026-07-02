//! Lazy PDF loading — parse only the objects our text pipeline reaches.
//!
//! lopdf's `load_mem` materializes every object (copying multi-MB image/font
//! bodies) and its parser is O(offset) per object. This module parses the classic
//! cross-reference table itself, then resolves *just* the text-reachable objects
//! with our own O(1)-offset [`ObjParser`] (byte-identical to lopdf, verified on the
//! corpus). References into `/XObject` (images/forms) and `/FontFile*` (glyph
//! programs) are never followed, so those bodies are never read.
//!
//! Scope: classic cross-reference *tables*, unencrypted. Cross-reference streams
//! and encrypted files return `None` and the caller falls back to eager loading —
//! correctness never depends on this fast path.

use std::collections::{BTreeMap, HashSet};

use lopdf::xref::{Xref, XrefEntry, XrefType};
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId};

use super::object_parser::ObjParser;

/// Keys whose referenced stream bodies we never consume; skipping them is what
/// makes lazy loading fast and does not change text output.
pub(super) fn is_skipped_key(key: &[u8]) -> bool {
    matches!(key, b"XObject" | b"FontFile" | b"FontFile2" | b"FontFile3")
}

/// Try to lazily load `data` into a document holding the full cross-reference
/// table and the resolved text-reachable objects. Returns `None` to signal the
/// caller should fall back to eager `load_mem`.
pub(super) fn try_load(data: &[u8]) -> Option<LoDoc> {
    let start = data.windows(5).position(|w| w == b"%PDF-")?;
    let buf = &data[start..];

    let xref_start = find_startxref(buf)?;
    let (xref, trailer) = parse_xref_chain(buf, xref_start)?;
    if trailer.get(b"Encrypt").is_ok() {
        return None; // needs lopdf's decryption pass
    }
    let root = trailer.get(b"Root").ok()?.as_reference().ok()?;

    let max_id = xref.entries.keys().copied().max().unwrap_or(0);
    let parser = ObjParser::new(buf, &xref);
    let objects = resolve_reachable(&parser, &xref, root);

    let mut doc = LoDoc::new();
    doc.version = "1.5".to_string();
    doc.max_id = max_id;
    let mut xref = xref;
    xref.size = max_id + 1;
    doc.reference_table = xref;
    doc.trailer = trailer;
    doc.objects = objects;
    Some(doc)
}

fn normal_offset(xref: &Xref, id: ObjectId) -> Option<usize> {
    match xref.get(id.0)? {
        XrefEntry::Normal { offset, generation } if *generation == id.1 => Some(*offset as usize),
        _ => None,
    }
}

/// Resolve the transitive closure of objects reachable from `root`, following all
/// references except the skipped keys. Objects that fail to parse are omitted
/// (as the eager loader drops unparseable objects); the validation pass reports
/// any reference to them.
fn resolve_reachable(parser: &ObjParser, xref: &Xref, root: ObjectId) -> BTreeMap<ObjectId, Object> {
    let mut objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut queued: HashSet<ObjectId> = HashSet::new();
    queued.insert(root);
    let mut stack = vec![root];

    while let Some(id) = stack.pop() {
        let Some(offset) = normal_offset(xref, id) else { continue };
        let Some((_, obj)) = parser.indirect_at(offset) else { continue };
        let mut children = Vec::new();
        collect_refs(&obj, &mut children);
        for child in children {
            if queued.insert(child) {
                stack.push(child);
            }
        }
        objects.insert(id, obj);
    }
    objects
}

/// Push every object reference in `obj` onto `out`, skipping references under a
/// skipped key (image/form XObjects, font programs).
pub(super) fn collect_refs(obj: &Object, out: &mut Vec<ObjectId>) {
    match obj {
        Object::Reference(id) => out.push(*id),
        Object::Array(items) => items.iter().for_each(|it| collect_refs(it, out)),
        Object::Dictionary(d) => collect_dict_refs(d, out),
        Object::Stream(s) => collect_dict_refs(&s.dict, out),
        _ => {}
    }
}

fn collect_dict_refs(dict: &Dictionary, out: &mut Vec<ObjectId>) {
    for (key, value) in dict.iter() {
        if is_skipped_key(key) {
            continue;
        }
        collect_refs(value, out);
    }
}

fn find_startxref(buf: &[u8]) -> Option<usize> {
    let tail_start = buf.len().saturating_sub(2048);
    let rel = buf[tail_start..].windows(9).rposition(|w| w == b"startxref")?;
    let after = tail_start + rel + 9;
    let digits: Vec<u8> = buf[after..]
        .iter()
        .copied()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .collect();
    std::str::from_utf8(&digits).ok()?.parse::<usize>().ok()
}

/// Parse a chain of classic cross-reference tables (following `/Prev`, newest
/// first so newer entries win). Returns `None` for cross-reference streams.
fn parse_xref_chain(buf: &[u8], start: usize) -> Option<(Xref, Dictionary)> {
    let mut xref = Xref::new(0, XrefType::CrossReferenceTable);
    let mut seen_ids: HashSet<u32> = HashSet::new();
    let mut seen_offsets: HashSet<usize> = HashSet::new();
    let mut top_trailer: Option<Dictionary> = None;
    let mut next = Some(start);

    while let Some(off) = next {
        if off >= buf.len() || !seen_offsets.insert(off) {
            break;
        }
        let section = &buf[off..];
        if !section.starts_with(b"xref") {
            return None; // cross-reference stream or malformed → fall back
        }
        let (entries, trailer_off) = parse_xref_table(section)?;
        for (id, entry) in entries {
            if seen_ids.insert(id) {
                if let XrefEntry::Normal { .. } = entry {
                    xref.insert(id, entry);
                }
            }
        }
        let trailer = parse_trailer_dict(&section[trailer_off..])?;
        next = trailer.get(b"Prev").ok().and_then(|p| p.as_i64().ok()).map(|p| p as usize);
        if top_trailer.is_none() {
            top_trailer = Some(trailer);
        }
    }
    Some((xref, top_trailer?))
}

/// Parse one classic `xref` section; returns its entries and the offset of the
/// following `trailer` keyword.
fn parse_xref_table(section: &[u8]) -> Option<(Vec<(u32, XrefEntry)>, usize)> {
    let mut entries = Vec::new();
    let mut i = 4; // past "xref"
    skip_eol(section, &mut i);
    loop {
        if section[i..].starts_with(b"trailer") {
            return Some((entries, i));
        }
        let line_end = section[i..].iter().position(|&b| b == b'\r' || b == b'\n')? + i;
        let header = std::str::from_utf8(&section[i..line_end]).ok()?;
        let mut it = header.split_whitespace();
        let sub_start: u32 = it.next()?.parse().ok()?;
        let count: u32 = it.next()?.parse().ok()?;
        i = line_end;
        skip_eol(section, &mut i);
        for k in 0..count {
            // 20-byte entries: "nnnnnnnnnn ggggg k\r\n".
            let entry = section.get(i..i + 20)?;
            let kind = entry[17];
            if kind == b'n' {
                let offset: u32 = std::str::from_utf8(&entry[0..10]).ok()?.trim().parse().ok()?;
                let generation: u16 = std::str::from_utf8(&entry[11..16]).ok()?.trim().parse().ok()?;
                entries.push((sub_start + k, XrefEntry::Normal { offset, generation }));
            } else {
                entries.push((sub_start + k, XrefEntry::Free));
            }
            i += 20;
        }
    }
}

/// Minimal trailer parse: extract `/Root`, `/Prev`, detect `/Encrypt`.
fn parse_trailer_dict(section: &[u8]) -> Option<Dictionary> {
    let start = section.windows(2).position(|w| w == b"<<")? + 2;
    let end = section[start..].windows(2).position(|w| w == b">>")? + start;
    let body = &section[start..end];
    let mut dict = Dictionary::new();
    if let Some(root) = find_reference(body, b"/Root") {
        dict.set("Root", Object::Reference(root));
    }
    if let Some(prev) = find_int(body, b"/Prev") {
        dict.set("Prev", Object::Integer(prev));
    }
    if body.windows(8).any(|w| w == b"/Encrypt") {
        dict.set("Encrypt", Object::Null);
    }
    Some(dict)
}

fn find_reference(body: &[u8], key: &[u8]) -> Option<ObjectId> {
    let pos = find_key(body, key)?;
    let text = std::str::from_utf8(&body[pos..]).ok()?;
    let mut it = text.split_whitespace();
    let num: u32 = it.next()?.parse().ok()?;
    let gen: u16 = it.next()?.parse().ok()?;
    if it.next()? != "R" {
        return None;
    }
    Some((num, gen))
}

fn find_int(body: &[u8], key: &[u8]) -> Option<i64> {
    let pos = find_key(body, key)?;
    std::str::from_utf8(&body[pos..]).ok()?.split_whitespace().next()?.parse().ok()
}

fn find_key(body: &[u8], key: &[u8]) -> Option<usize> {
    body.windows(key.len()).position(|w| w == key).map(|p| p + key.len())
}

fn skip_eol(section: &[u8], i: &mut usize) {
    while *i < section.len() && (section[*i] == b'\r' || section[*i] == b'\n') {
        *i += 1;
    }
}
