//! An experimental functional reactive programming library
//!
//! *Carboxyl* provides primitives for functional reactive programming in Rust.
//! It is heavily influenced by the
//! [Sodium](https://github.com/SodiumFRP/sodium/) libraries.
//!
//! Functional reactive programming (FRP) is a paradigm that effectively fixes
//! the issues present in the traditional observer pattern approach to event
//! handling. It uses a set of compositional primitives to model the dependency
//! graph of a reactive system. These primitives essentially provide a type- and
//! thread-safe, compositional abstraction around mutable state in your
//! application to avoid the pitfalls normally associated with it.
//!
//! If you want to learn more about FRP in general, check out [the Sodium
//! blog](http://blog.reactiveprogramming.org).
//!
//!
//! # Functional reactive primitives
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
//!
//! # Example
//!
//! Here is some code that demonstrates what you can do with streams and cells:
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
//! You may have noticed that certain primitives take a function as an argument.
//! There is a limitation on what kind of functions can and should be used here.
//! In general, as FRP provides an abstraction around mutable state, they should
//! be pure functions (i.e. free of side effects).
//!
//! For the most part this is guaranteed by Rust's type system. A static
//! function with a matching signature always works. A closure though is very
//! restricted: it must not borrow its environment, as it is impossible to
//! satisfy the lifetime requirements for that. So you can only move stuff into
//! it from the environment. However, the moved contents of the closure may also
//! not be altered, which is guaranteed by the `Fn(…) -> …)` trait bound.
//!
//! However, both closures and functions could still have side effects such as
//! I/O, changing shared mutable state via `Arc` pointers, etc. While Rust's
//! type system cannot prevent this, you should generally not pass such
//! functions to the FRP primitives, as they break the benefits you get from
//! using FRP. (Except temporary print statements for debugging.)

#![feature(unboxed_closures)]
#![allow(unstable)]
#![warn(missing_docs)]

#[cfg(test)]
extern crate test;

pub use primitives::{Stream, Cell, Sink, lift2};

mod subject;
mod primitives;
