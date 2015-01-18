//! An experimental functional reactive programming library
//!
//! `carboxyl` provides primitives for functional reactive programming in Rust.
//! It is heavily influenced by the
//! [Sodium](https://github.com/SodiumFRP/sodium/) libraries.
//!
//! Functional reactive programming (FRP) is a paradigm that effectively fixes
//! the issues present in the traditional observer pattern approach to event
//! handling. It uses a set of compositional primitives to model the dependency
//! graph of a reactive system. If you want to learn more about FRP, check out
//! [the Sodium blog](http://blog.reactiveprogramming.org).
//!
//! This library provides two basic types: `Stream` and `Cell`. A stream is a
//! discrete sequence of events, a cell is a container for values that change
//! (discretely) over time.
//!
//! The FRP primitive functions are mostly implemented as methods of the basic
//! types to ease method chaining, except for `lift2` which does not really
//! belong to any type in particular.
//!
//! In addition, the `Sink` type allows one to create a stream of events by
//! dumping values into it. It is the only way to create an event from scratch,
//! i.e. without using any of the other primitives.
//!
//! Here is a simple example of what you can do with streams and cells:
//!
//! ```
//! # // NOTE: If you change this example, please update the README.md
//! # // accordingly, so that they remain in sync!
//! use carboxyl::Sink;
//!
//! // A new sink with a stream
//! let sink = Sink::new();
//! let stream = sink.stream();
//!
//! // Make a cell by holding the last event in a stream
//! let cell = stream.hold(3);
//! assert_eq!(cell.sample(), 3);
//!
//! // Send a value into the sink
//! sink.send(5);
//!
//! // The cell gets updated accordingly
//! assert_eq!(cell.sample(), 5);
//!
//! // Now map it to something else
//! let squares = stream.map(|x| x * x).hold(0);
//! sink.send(4);
//! assert_eq!(squares.sample(), 16);
//!
//! // Or filter it
//! let negatives = stream.filter(|&x| x < 0).hold(0);
//! sink.send(4); // This won't arrive
//! assert_eq!(negatives.sample(), 0);
//! sink.send(-3); // but this will!
//! assert_eq!(negatives.sample(), -3);
//! ```
//!
//! There are some other methods on streams and cells, that you can find in
//! their respective APIs.
//!
//! Note that all these objects are `Send + Sync + Clone`. This means you can
//! easily pass them around in your code, make clones, give them to another
//! thread, and they will still be updated correctly.
//!
//! As you can see, these functional reactive primitives essentially provide a
//! type- and thread-safe, compositional abstraction around mutable state in
//! your application to avoid the pitfalls associated with it.

#![feature(unboxed_closures)]
#![allow(unstable)]
#![warn(missing_docs)]

#[cfg(test)]
extern crate test;

pub use primitives::{Stream, Cell, Sink, lift2};

mod subject;
mod primitives;
