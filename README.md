[![Build Status](https://img.shields.io/travis/aepsil0n/carboxyl.svg)](https://travis-ci.org/aepsil0n/carboxyl)
[![](https://img.shields.io/crates/v/carboxyl.svg)](https://crates.io/crates/carboxyl)

*Carboxyl* is a library for functional reactive programming in Rust, a
functional and composable approach to handle events in interactive
applications.
[To the docs…](http://www.rust-ci.org/aepsil0n/carboxyl/doc/carboxyl/)


## Usage example

Here is a simple example of how you can use the primitives provided by
*Carboxyl*. First of all, events can be sent into a *sink*. From a sink one can
create a *stream* of events. Streams can also be filtered, mapped and merged. A
*cell* is an abstraction of a value that may change over time. One can e.g.
hold the last event from a stream in a cell.

```rust
use carboxyl::Sink;

let sink = Sink::new();
let stream = sink.stream();
let cell = stream.hold(3);

// The current value of the cell is initially 3
assert_eq!(cell.sample(), 3);

// When we fire an event, the cell get updated accordingly
sink.send(5);
assert_eq!(cell.sample(), 5);
```

One can also directly iterate over the stream instead of holding it in a
cell:

```rust
let mut events = stream.events();
sink.send(4);
assert_eq!(events.next(), Some(4));
```

Streams and cells can be combined using various primitives. We can map a stream
to another stream using a function:

```rust
let squares = stream.map(|x| x * x).hold(0);
sink.send(4);
assert_eq!(squares.sample(), 16);
```

Or we can filter a stream to create a new one that only contains events that
satisfy a certain predicate:

```rust
let negatives = stream.filter(|&x| x < 0).hold(0);

// This won't arrive at the cell.
sink.send(4);
assert_eq!(negatives.sample(), 0);

// But this will!
sink.send(-3);
assert_eq!(negatives.sample(), -3);
```

There are a couple of other primitives to compose streams and cells:

- `merge` two streams of events of the same type.
- Make a `snapshot` of a cell, whenever a stream fires an event.
- `lift!` an ordinary function to a function on cells.
- `switch` between different cells using a cell containing a cell.

See the [documentation](http://www.rust-ci.org/aepsil0n/carboxyl/doc/carboxyl/)
for details.


## Limitations

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


## License

Copyright 2014, 2015 Eduard Bopp.

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU Lesser General Public License or GNU General Public
License as published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but **without
any warranty**; without even the implied warranty of **merchantability** or
**fitness for a particular purpose**.  See the GNU General Public License for
more details.

You should have received a copy of the GNU General Public License and the GNU
Lesser General Public License along with this program. If not, see
http://www.gnu.org/licenses/.
