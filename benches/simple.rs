//! Simple benchmarks

use carboxyl::Sink;
use criterion::{criterion_group, criterion_main, Criterion, Bencher};

fn bench_chain(b: &mut Bencher<'_>) {
    let sink: Sink<i32> = Sink::new();
    let _ = sink.stream()
        .map(|x| x + 4)
        .filter(|&x| x < 4)
        .merge(&sink.stream().map(|x| x * 5))
        .hold(15);
    b.iter(|| sink.send(-5));
}

fn bench_fn(c: &mut Criterion) {
    c.bench_function("simple", bench_chain);
}

criterion_group!(benches, bench_fn);
criterion_main!(benches);
