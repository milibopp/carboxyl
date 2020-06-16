//! FRP benchmarks from https://github.com/tsurucapital/frp-benchmarks

use carboxyl::{Sink, Stream};
use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use rand::{rngs::StdRng, seq::IteratorRandom, SeedableRng};

/// First-order benchmark.
///
/// Generate `n_sinks` `Stream<String>`. Create a network that prints the output
/// of each stream. At each network step push a string (the step number
/// formatted as a string) into 10 randomly-selected nodes.
///
/// Benchmark the time required for `n_steps` steps.
fn first_order(n_sinks: usize, n_steps: usize, b: &mut Bencher<'_>) {
    // Setup network
    let sinks: Vec<Sink<String>> = (0..n_sinks).map(|_| Sink::new()).collect();
    let _printers: Vec<Stream<()>> = sinks
        .iter()
        .map(|sink| {
            sink.stream().map(|s| {
                format!("{}", s);
            })
        })
        .collect();

    // Feed events
    let mut rng = StdRng::from_entropy();
    b.iter(|| {
        for k in 0..n_steps {
            let s = format!("{}", k);
            for sink in sinks.iter().choose_multiple(&mut rng, 10) {
                sink.send(s.clone());
            }
        }
    });
}

/// A small reference benchmark to do the same amount of actual work without FRP
fn first_order_1k_ref(b: &mut Bencher<'_>) {
    let mut rng = StdRng::from_entropy();
    b.iter(|| {
        for i in 0..1_000 {
            for _k in (0..1_000).choose_multiple(&mut rng, 10) {
                format!("{}", i);
            }
        }
    });
}

fn bench_fn(c: &mut Criterion) {
    c.bench_function("first order 1k reference", |b| first_order_1k_ref(b));
    c.bench_function("first order 100", |b| first_order(1_000, 100, b));
    c.bench_function("first order 1k", |b| first_order(1_000, 1_000, b));
    c.bench_function("first order 10k", |b| first_order(1_000, 10_000, b));
}

criterion_group!(benches, bench_fn);
criterion_main!(benches);
