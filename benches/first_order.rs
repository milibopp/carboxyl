//! FRP benchmarks from https://github.com/tsurucapital/frp-benchmarks
#![feature(test)]

extern crate test;
extern crate rand;
extern crate carboxyl;

use test::Bencher;
use rand::{XorShiftRng, sample};
use carboxyl::{Sink, Stream};


/// First-order benchmark.
///
/// Generate `n_sinks` `Stream<String>`. Create a network that prints the output
/// of each stream. At each network step push a string (the step number
/// formatted as a string) into 10 randomly-selected nodes.
///
/// Benchmark the time required for `n_steps` steps.
fn first_order(n_sinks: usize, n_steps: usize, b: &mut Bencher) {
    // Setup network
    let sinks: Vec<Sink<String>> = (0..n_sinks).map(|_| Sink::new()).collect();
    let _printers: Vec<Stream<()>> = sinks.iter()
        .map(|sink| sink.stream().map(|s| { format!("{}", s); }))
        .collect();

    // Feed events
    let mut rng = XorShiftRng::new_unseeded();
    b.iter(|| for k in 0..n_steps {
        let s = format!("{}", k);
        for sink in sample(&mut rng, sinks.iter(), 10) {
            sink.send(s.clone());
        }
    });
}

#[bench]
fn first_order_100(b: &mut Bencher) {
    first_order(1_000, 100, b);
}

#[bench]
fn first_order_1k(b: &mut Bencher) {
    first_order(1_000, 1_000, b);
}

#[bench]
fn first_order_10k(b: &mut Bencher) {
    first_order(1_000, 10_000, b);
}

/// A small reference benchmark to do the same amount of actual work without FRP
#[bench]
fn first_order_1k_ref(b: &mut Bencher) {
    let mut rng = XorShiftRng::new_unseeded();
    b.iter(|| for i in 0..1_000 {
        for _k in sample(&mut rng, 0..1_000, 10) {
            format!("{}", i);
        }
    });
}
