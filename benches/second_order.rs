//! FRP benchmarks from https://github.com/tsurucapital/frp-benchmarks
#![feature(test)]

extern crate test;
extern crate rand;
extern crate carboxyl;

use test::Bencher;
use rand::{XorShiftRng, sample};
use carboxyl::Sink;


/// Second-order benchmark.
///
/// Generate `n_sinks` `Stream<()>`, then for each stream create a `Cell<i32>`
/// that counts the number of firings. Create a `Stream<Cell<i32>>` that every
/// 10 network steps sequentially moves to the next cell. Create a `Cell<i32>`
/// from this stream. At each network step, fire 10 `Stream<()>` at random,
/// then print the current value of the `Cell<i32>`.
///
/// Benchmark the time required for `n_steps` steps.
fn second_order(n_sinks: usize, n_steps: usize, b: &mut Bencher) {
    // Setup network
    let stepper = Sink::<usize>::new();
    let sinks = (0..n_sinks)
        .map(|_| Sink::<()>::new())
        .collect::<Vec<_>>();
    let counters = sinks.iter()
        .map(|sink| sink.stream().scan(0, |n, _| n + 1))
        .collect::<Vec<_>>();
    let walker = {
        let counters = counters.clone();
        stepper.stream().map(move |k| counters[k / 10].clone())
    };
    let cell = walker.hold(counters[0].clone()).switch();

    // Feed events
    let mut rng = XorShiftRng::new_unseeded();
    b.iter(|| for i in 0..n_steps {
        stepper.send(i);
        for sink in sample(&mut rng, sinks.iter(), 10) {
            sink.send(());
        }
        format!("{}", cell.sample());
    });
}

#[bench]
fn second_order_100(b: &mut Bencher) {
    second_order(1_000, 100, b);
}

#[bench]
fn second_order_1k(b: &mut Bencher) {
    second_order(1_000, 1_000, b);
}

#[bench]
fn second_order_10k(b: &mut Bencher) {
    second_order(1_000, 10_000, b);
}
