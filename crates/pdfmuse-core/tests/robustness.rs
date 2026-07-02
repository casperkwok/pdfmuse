//! Robustness: `parse()` must never panic — malformed input degrades to `Err`
//! or partial output, never a crash (principle 4). Complements cargo-fuzz with a
//! deterministic, stable-toolchain gate over mutated corpus files + random bytes.

use std::panic;

/// Tiny deterministic PRNG (xorshift) — reproducible, no external deps, no time.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn corpus() -> Vec<Vec<u8>> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/corpus");
    ["hello.pdf", "table.pdf", "cjk.pdf", "encrypted.pdf", "sample.docx"]
        .iter()
        .filter_map(|f| std::fs::read(format!("{dir}/{f}")).ok())
        .collect()
}

/// Parse under catch_unwind; returns false if it panicked.
fn no_panic(data: &[u8]) -> bool {
    let owned = data.to_vec();
    panic::catch_unwind(|| {
        let _ = pdfmuse_core::parse(&owned, None);
        let _ = pdfmuse_core::parse(&owned, Some(pdfmuse_core::Format::Pdf));
        let _ = pdfmuse_core::parse(&owned, Some(pdfmuse_core::Format::Docx));
    })
    .is_ok()
}

#[test]
fn never_panics_on_truncations() {
    for data in corpus() {
        // Every 1/32 cut point, plus the empty and near-empty prefixes.
        for k in 0..=32 {
            let cut = data.len() * k / 32;
            assert!(no_panic(&data[..cut]), "panicked on {cut}-byte truncation");
        }
    }
}

#[test]
fn never_panics_on_bit_flips() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for data in corpus() {
        if data.is_empty() {
            continue;
        }
        for _ in 0..300 {
            let mut m = data.clone();
            // Flip 1..=8 random bytes to arbitrary values.
            for _ in 0..1 + rng.below(8) {
                let i = rng.below(m.len());
                m[i] = rng.next() as u8;
            }
            assert!(no_panic(&m), "panicked on a bit-flip mutation");
        }
    }
}

#[test]
fn never_panics_on_random_and_edge_bytes() {
    let mut rng = Rng(0xD1B54A32D192ED03);
    // Random buffers, some with PDF/ZIP magic to reach deeper code paths.
    for _ in 0..2000 {
        let n = rng.below(4096);
        let mut buf: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        match rng.below(4) {
            0 => buf = [&b"%PDF-1.5\n"[..], &buf].concat(),
            1 => buf = [&b"PK\x03\x04"[..], &buf].concat(),
            2 => buf.extend_from_slice(b"\nstartxref\n0\n%%EOF"),
            _ => {}
        }
        assert!(no_panic(&buf), "panicked on random buffer");
    }
    // Explicit edge cases.
    for edge in [&b""[..], b"%PDF-", b"%PDF-1.7", b"PK\x03\x04", b"xref", b"trailer<<>>"] {
        assert!(no_panic(edge), "panicked on edge input {edge:?}");
    }
}
