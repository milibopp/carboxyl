`carboxyl` is a library for functional reactive programming in Rust.


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
