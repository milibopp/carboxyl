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

#![feature(unboxed_closures)]
#![allow(unstable)]
#![warn(missing_docs)]

pub use primitives::{Stream, Cell, Sink, lift2};

mod subject;
mod primitives;
