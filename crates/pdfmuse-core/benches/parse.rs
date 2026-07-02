//! Criterion micro-benchmarks over the golden corpus — a regression signal for
//! parse throughput. Real-file comparison vs pdfplumber/PyMuPDF is in
//! `benches/compare.py`. Run: `cargo bench -p pdfmuse-core`.
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn parse_corpus(c: &mut Criterion) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/corpus");
    let mut group = c.benchmark_group("parse");
    for name in ["hello.pdf", "table.pdf", "cjk.pdf", "sample.docx"] {
        let Ok(data) = std::fs::read(format!("{dir}/{name}")) else { continue };
        group.bench_function(name, |b| b.iter(|| pdfmuse_core::parse(black_box(&data), None)));
    }
    group.finish();
}

criterion_group!(benches, parse_corpus);
criterion_main!(benches);
