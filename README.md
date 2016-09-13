[![Stories in Ready](https://badge.waffle.io/aepsil0n/carboxyl.png?label=ready&title=Ready)](https://waffle.io/aepsil0n/carboxyl)
[![Build Status](https://img.shields.io/travis/aepsil0n/carboxyl.svg)](https://travis-ci.org/aepsil0n/carboxyl)
[![](https://img.shields.io/crates/v/carboxyl.svg)](https://crates.io/crates/carboxyl)

*Carboxyl* is a library for functional reactive programming in Rust, a
functional and composable approach to handle events in interactive
applications. Read more [in the docs…][docs]

[docs]: https://aepsil0n.github.io/carboxyl/


## Usage example

Here is a simple example of how you can use the primitives provided by
*Carboxyl*. First of all, events can be sent into a *sink*. From a sink one can
create a *stream* of events. Streams can also be filtered, mapped and merged. A
*signal* is an abstraction of a value that may change over time. One can e.g.
hold the last event from a stream in a signal.

```rust
extern crate carboxyl;

let sink = carboxyl::Sink::new();
let stream = sink.stream();
let signal = stream.hold(3);

// The current value of the signal is initially 3
assert_eq!(signal.sample(), 3);

// When we fire an event, the signal get updated accordingly
sink.send(5);
assert_eq!(signal.sample(), 5);
```

One can also directly iterate over the stream instead of holding it in a
signal:

```rust
extern crate carboxyl;
let sink = carboxyl::Sink::new();
let stream = sink.stream();

let mut events = stream.events();
sink.send(4);
assert_eq!(events.next(), Some(4));
```

Streams and signals can be combined using various primitives. We can map a
stream to another stream using a function:

```rust
extern crate carboxyl;
let sink = carboxyl::Sink::new();
let stream = sink.stream();

let squares = stream.map(|x| x * x).hold(0);
sink.send(4);
assert_eq!(squares.sample(), 16);
```

Or we can filter a stream to create a new one that only contains events that
satisfy a certain predicate:

```rust
extern crate carboxyl;
let sink = carboxyl::Sink::new();
let stream = sink.stream();

let negatives = stream.filter(|&x| x < 0).hold(0);

// This won't arrive at the signal.
sink.send(4);
assert_eq!(negatives.sample(), 0);

// But this will!
sink.send(-3);
assert_eq!(negatives.sample(), -3);
```

There are a couple of other primitives to compose streams and signals:

- `merge` two streams of events of the same type.
- Make a `snapshot` of a signal, whenever a stream fires an event.
- `lift!` an ordinary function to a function on signals.
- `switch` between different signals using a signal containing a signal.

See the [documentation][docs] for details.


## License

Copyright 2014, 2015, 2016 Emilia Bopp.

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
