//! Micro-benchmarks for [`TraceContext`].
//!
//! Ran on every publish (inject) and every receive (extract) through
//! NATS headers. Kept here so any regression in parse/format cost
//! surfaces against a baseline rather than silently accumulating.

use choreo_core::value_objects::TraceContext;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

const W3C_EXAMPLE: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

fn parse(c: &mut Criterion) {
    c.bench_function("TraceContext::parse(valid)", |b| {
        b.iter(|| TraceContext::parse(black_box(W3C_EXAMPLE)).unwrap());
    });
}

fn format(c: &mut Criterion) {
    let ctx = TraceContext::parse(W3C_EXAMPLE).unwrap();
    c.bench_function("TraceContext::to_header", |b| {
        b.iter(|| black_box(ctx.to_header()));
    });
}

fn generate(c: &mut Criterion) {
    c.bench_function("TraceContext::generate", |b| {
        b.iter(TraceContext::generate);
    });
}

fn roundtrip(c: &mut Criterion) {
    c.bench_function("TraceContext::parse + to_header", |b| {
        b.iter(|| {
            let ctx = TraceContext::parse(black_box(W3C_EXAMPLE)).unwrap();
            black_box(ctx.to_header())
        });
    });
}

criterion_group!(benches, parse, format, generate, roundtrip);
criterion_main!(benches);
