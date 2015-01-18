`carboxyl` is a library for functional reactive programming in Rust.

- [Crate on crates.io](https://crates.io/crates/carboxyl)
- [Documentation on Rust CI](http://www.rust-ci.org/aepsil0n/carboxyl/doc/carboxyl/)
- [Travis CI: ![Build Status](https://travis-ci.org/aepsil0n/carboxyl.svg?branch=master)](https://travis-ci.org/aepsil0n/carboxyl)


## About

`carboxyl` provides primitives for functional reactive programming.  It is
heavily influenced by the [Sodium](https://github.com/SodiumFRP/sodium/)
libraries.

Functional reactive programming (FRP) is a paradigm that effectively fixes the
issues present in the traditional observer pattern approach to event handling.
It uses a set of compositional primitives to model the dependency graph of a
reactive system. If you want to learn more about FRP, check out [the Sodium
blog](http://blog.reactiveprogramming.org).

This library provides two basic types: `Stream` and `Cell`. A stream is a
discrete sequence of events, a cell is a container for values that change
(discretely) over time.


### API

The FRP primitive functions are mostly implemented as methods of the basic types
to ease method chaining, except for `lift2` which does not really belong to any
type in particular.

In addition, the `Sink` type allows one to create a stream of events by dumping
values into it. It is the only way to create an event from scratch, i.e. without
using any of the other primitives.

For details, please refer to the
[documentation](http://www.rust-ci.org/aepsil0n/carboxyl/doc/carboxyl/).


### Example

Here is a simple example of how you can use the primitives provided by
`carboxyl`:

```rust
use carboxyl::Sink;

// A new sink with a stream
let sink = Sink::new();
let stream = sink.stream();

// Make a cell by holding the last event in a stream
let cell = stream.hold(3);
assert_eq!(cell.sample(), 3);

// Send a value into the sink
sink.send(5);

// The cell gets updated accordingly
assert_eq!(cell.sample(), 5);

// Now map it to something else
let squares = stream.map(|x| x * x).hold(0);
sink.send(4);
assert_eq!(squares.sample(), 16);

// Or filter it
let negatives = stream.filter(|&x| x < 0).hold(0);
sink.send(4); // This won't arrive
assert_eq!(negatives.sample(), 0);
sink.send(-3); // but this will!
assert_eq!(negatives.sample(), -3);
```


### Limitations

This library is fairly experimental and currently has some limitations:

- There are no strong guarantees about the order of events if used
  asynchronously. While events from the same source are guaranteed to arrive in
  order, events from different asynchronous sources arrive in an undefined
  order. This may be improved by introducing the notion of transactions in the
  internals, but I have not looked into that yet.
- The implementation relies on reference counting and vtable dispatch a lot.
  While this is necessary for it to work to some degree, I think it may be
  slower than it could potentially be. For reference, it takes about half a
  microsecond to dispatch an event through a simple dependency graph (see the
  benchmark in the source). While this is not too bad on its own, I suspect that
  in real-life applications, the event dispatch will suffer a lot of cache
  misses. I hope to find a better way to work around or with the limitations
  imposed by Rust's type system in the future.

Furthermore, it has not been used in any application yet. For all these reasons,
I would be naturally very glad about feedback.
