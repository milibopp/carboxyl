//! *Carboxyl* provides primitives for functional reactive programming in Rust.
//! It draws inspiration from the [Sodium][sodium] libraries and Push-Pull FRP,
//! as described by [Elliott (2009)][elliott_push_pull].
//!
//! [sodium]: https://github.com/SodiumFRP/sodium/
//! [elliott_push_pull]: http://conal.net/papers/push-pull-frp/push-pull-frp.pdf
//!
//!
//! # Overview
//!
//! Functional reactive programming (FRP) is a composable and modular
//! abstraction for creating dynamic and reactive systems. In its most general
//! form it models these systems as a composition of two basic primitives:
//! *streams* are a series of singular events and *signals* are continuously
//! changing values.
//!
//! *Carboxyl* is an imperative, hybrid push- and pull-based implementation of
//! FRP. Streams and the discrete components of signals are data-driven, i.e.
//! whenever an event occurs the resulting changes are propagated to everything
//! that depends on it.
//!
//! However, the continuous components of signals are demand-driven. Internally,
//! *Carboxyl* stores the state of a signal as a function. This function has to
//! be evaluated by consumers of a signal to obtain a concrete value.
//!
//! Nonetheless, *Carboxyl* has no explicit notion of time. Its signals are
//! functions that can be evaluated at any time, but they do not carry any
//! inherent notion of time. Synchronization and atomicity is achieved by a
//! transaction system.
//!
//!
//! # Functional reactive primitives
//!
//! This library provides two basic types: `Stream` and `Signal`. A stream is a
//! discrete sequence of events, a signal is a container for values that change
//! (discretely) over time.
//!
//! The FRP primitives are mostly implemented as methods of the basic types to
//! ease method chaining, except for the various lifting functions, as they do
//! not really belong to any type in particular.
//!
//! In addition, the `Sink` type allows one to create a stream of events by
//! sending values into it. It is the only way to create a stream from scratch,
//! i.e. without using any of the other primitives.
//!
//!
//! # Usage example
//!
//! Here is a simple example of how you can use the primitives provided by
//! *Carboxyl*. First of all, events can be sent into a *sink*. From a sink one
//! can create a *stream* of events. Streams can also be filtered, mapped and
//! merged. One can e.g. hold the last event from a stream as a signal.
//!
//! ```
//! use carboxyl::Sink;
//!
//! let sink = Sink::new();
//! let stream = sink.stream();
//! let signal = stream.hold(3);
//!
//! // The current value of the signal is initially 3
//! assert_eq!(signal.sample(), 3);
//!
//! // When we fire an event, the signal get updated accordingly
//! sink.send(5);
//! assert_eq!(signal.sample(), 5);
//! ```
//!
//! One can also directly iterate over the stream instead of holding it in a
//! signal:
//!
//! ```
//! # use carboxyl::Sink;
//! # let sink = Sink::new();
//! # let stream = sink.stream();
//! let mut events = stream.events();
//! sink.send(4);
//! assert_eq!(events.next(), Some(4));
//! ```
//!
//! Streams and signals can be combined using various primitives. We can map a
//! stream to another stream using a function:
//!
//! ```
//! # use carboxyl::Sink;
//! # let sink: Sink<i32> = Sink::new();
//! # let stream = sink.stream();
//! let squares = stream.map(|x| x * x).hold(0);
//! sink.send(4);
//! assert_eq!(squares.sample(), 16);
//! ```
//!
//! Or we can filter a stream to create a new one that only contains events that
//! satisfy a certain predicate:
//!
//! ```
//! # use carboxyl::Sink;
//! # let sink: Sink<i32> = Sink::new();
//! # let stream = sink.stream();
//! let negatives = stream.filter(|&x| x < 0).hold(0);
//!
//! // This won't arrive at the signal.
//! sink.send(4);
//! assert_eq!(negatives.sample(), 0);
//!
//! // But this will!
//! sink.send(-3);
//! assert_eq!(negatives.sample(), -3);
//! ```
//!
//! There are some other methods on streams and signals, that you can find in
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
//! I/O, changing mutable state via `Mutex` or `RefCell`, etc. While Rust's type
//! system cannot prevent this, you should generally not pass such functions to
//! the FRP primitives, as they break the benefits you get from using FRP.
//! (An exception here is debugging output.)

#![feature(arc_weak, fnbox)]
#![cfg_attr(test, feature(test))]
#![warn(missing_docs)]

#[cfg(test)]
extern crate test;
#[cfg(test)]
extern crate rand;
#[cfg(test)]
extern crate quickcheck;
#[macro_use(lazy_static)]
extern crate lazy_static;

pub use stream::{ Sink, Stream };
pub use signal::{ Signal, SignalMut };

mod transaction;
mod source;
mod pending;
mod readonly;
mod stream;
mod signal;
#[macro_use]
pub mod lift;
#[cfg(test)]
mod testing;
