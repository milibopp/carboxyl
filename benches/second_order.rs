//! FRP benchmarks from https://github.com/tsurucapital/frp-benchmarks

use rand::{SeedableRng, seq::IteratorRandom, rngs::StdRng};
use carboxyl::Sink;
use criterion::{criterion_group, criterion_main, Criterion, Bencher};

/// Second-order benchmark.
///
/// Generate `n_sinks` `Stream<()>`, then for each stream create a `Signal<i32>`
/// that counts the number of firings. Create a `Stream<Signal<i32>>` that every
/// 10 network steps sequentially moves to the next signal. Create a
/// `Signal<i32>` from this stream. At each network step, fire 10 `Stream<()>`
/// at random, then print the current value of the `Signal<i32>`.
///
/// Benchmark the time required for `n_steps` steps.
fn second_order(n_sinks: usize, n_steps: usize, b: &mut Bencher<'_>) {
    // Setup network
    let stepper = Sink::<usize>::new();
    let sinks = (0..n_sinks)
        .map(|_| Sink::<()>::new())
        .collect::<Vec<_>>();
    let counters = sinks.iter()
        .map(|sink| sink.stream().fold(0, |n, _| n + 1))
        .collect::<Vec<_>>();
    let walker = {
        let counters = counters.clone();
        stepper.stream().map(move |k| counters[k / 10].clone())
    };
    let signal = walker.hold(counters[0].clone()).switch();

    // Feed events
    let mut rng = StdRng::from_entropy();
    b.iter(|| for i in 0..n_steps {
        stepper.send(i);
        for sink in sinks.iter().choose_multiple(&mut rng, 10) {
            sink.send(());
        }
        format!("{}", signal.sample());
    });
}

fn bench_fn(c: &mut Criterion) {
    c.bench_function("second order 100", |b| second_order(1_000, 100, b));
    c.bench_function("second order 1k", |b| second_order(1_000, 1_000, b));
    c.bench_function("second order 10k", |b| second_order(1_000, 10_000, b));
}

criterion_group!(benches, bench_fn);
criterion_main!(benches);
