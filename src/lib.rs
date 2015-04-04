//! *Carboxyl* provides primitives for functional reactive programming in Rust.
//! It is heavily influenced by the
//! [Sodium](https://github.com/SodiumFRP/sodium/) libraries.
//!
//! Functional reactive programming (FRP) is a paradigm that effectively fixes
//! the issues present in the traditional observer pattern approach to event
//! handling. It uses a set of compositional primitives to model the dependency
//! graph of a reactive system. These primitives essentially provide a type- and
//! thread-safe, compositional abstraction around events and  mutable state in
//! your application to avoid the pitfalls normally associated with it.
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
//! # Usage example
//!
//! Here is a simple example of how you can use the primitives provided by
//! *Carboxyl*. First of all, events can be sent into a *sink*. From a sink one
//! can create a *stream* of events. Streams can also be filtered, mapped and
//! merged. A *cell* is an abstraction of a value that may change over time. One
//! can e.g.  hold the last event from a stream in a cell.
//!
//! ```
//! use carboxyl::Sink;
//!
//! let sink = Sink::new();
//! let stream = sink.stream();
//! let cell = stream.hold(3);
//!
//! // The current value of the cell is initially 3
//! assert_eq!(cell.sample(), 3);
//!
//! // When we fire an event, the cell get updated accordingly
//! sink.send(5);
//! assert_eq!(cell.sample(), 5);
//! ```
//!
//! One can also directly iterate over the stream instead of holding it in a
//! cell:
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
//! Streams and cells can be combined using various primitives. We can map a
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
//! // This won't arrive at the cell.
//! sink.send(4);
//! assert_eq!(negatives.sample(), 0);
//!
//! // But this will!
//! sink.send(-3);
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

#![feature(alloc, std_misc)]
#![cfg_attr(test, feature(test))]
#![warn(missing_docs)]

#[cfg(test)]
extern crate test;

use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::ops::Deref;
use subject::{
    Subject, Source, Mapper, WrapArc, Snapper, Merger, Filter, Holder, Updates,
    WeakSnapperWrapper, SamplingSubject, CellSwitcher, ChannelBuffer, LoopCell,
    LoopCellEntry, Listener,
};
use transaction::commit;

mod transaction;
mod subject;
#[macro_use]
pub mod lift;


/// An event sink.
///
/// This primitive is a way of generating streams of events. One can send
/// input values into a sink and generate a stream that fires all these inputs
/// as events:
///
/// ```
/// # use carboxyl::Sink;
/// // A new sink
/// let sink = Sink::new();
///
/// // Make an iterator over a stream.
/// let mut events = sink.stream().events();
///
/// // Send a value into the sink
/// sink.send(5);
///
/// // The stream
/// assert_eq!(events.next(), Some(5));
/// ```
///
/// You can also feed a sink with an iterator:
///
/// ```
/// # use carboxyl::Sink;
/// # let sink = Sink::new();
/// # let mut events = sink.stream().events();
/// sink.feed(20..40);
/// assert_eq!(events.take(4).collect::<Vec<_>>(), vec![20, 21, 22, 23]);
/// ```
///
/// # Asynchronous calls
///
/// It is possible to send events into the sink asynchronously using the methods
/// `send_async` and `feed_async`. Note though, that this will void some
/// guarantees on the order of events. In the following example, it is unclear,
/// which event is the first in the stream:
///
/// ```
/// # use carboxyl::Sink;
/// let sink = Sink::new();
/// let mut events = sink.stream().events();
/// sink.send_async(13);
/// sink.send_async(22);
/// let first = events.next().unwrap();
/// assert!(first == 13 || first == 22);
/// ```
///
/// `feed_async` provides a workaround, as it preserves the order of events from
/// the iterator. However, any event sent into the sink after a call to it, may
/// come at any point between the iterator events.
pub struct Sink<A> {
    source: Arc<Mutex<Source<A>>>,
}

impl<A> Clone for Sink<A> {
    fn clone(&self) -> Sink<A> {
        Sink { source: self.source.clone() }
    }
}

impl<A: Send + Sync + Clone + 'static> Sink<A> {
    /// Create a new sink.
    pub fn new() -> Sink<A> {
        Sink { source: Arc::new(Mutex::new(Source::new())) }
    }

    /// Send a value into the sink.
    ///
    /// When a value is sent into the sink, an event is fired in all dependent
    /// streams.
    pub fn send(&self, a: A) {
        commit((), move |_| {
            self.source.lock()
                .ok().expect("Sink::send")
                .send(a)
        })
    }

    /// Asynchronous send.
    ///
    /// Same as `send`, but it spawns a new thread to process the updates to
    /// dependent streams and cells.
    pub fn send_async(&self, a: A) {
        let clone = self.clone();
        thread::spawn(move || clone.send(a));
    }

    /// Feed values from an iterator into the sink.
    ///
    /// This method feeds events into the sink from an iterator.
    pub fn feed<I: Iterator<Item=A>>(&self, iterator: I) {
        for event in iterator {
            self.send(event);
        }
    }

    /// Asynchronous feed.
    ///
    /// This is the same as `feed`, but it does not block, since it spawns the
    /// feeding as a new task. This is useful, if the provided iterator is large
    /// or even infinite (e.g. an I/O event loop).
    pub fn feed_async<I: Iterator<Item=A> + Send + 'static>(&self, iterator: I) {
        let clone = self.clone();
        thread::spawn(move || clone.feed(iterator));
    }

    /// Generate a stream that fires all events sent into the sink.
    pub fn stream(&self) -> Stream<A> {
        Stream { source: commit((), |_| self.source.wrap_as_subject()) }
    }
}


/// A stream of discrete events.
///
/// This occasionally fires an event of type `A` down its dependency graph.
pub struct Stream<A> {
    source: Arc<Mutex<Box<Subject<A> + 'static>>>,
}

impl<A> Clone for Stream<A> {
    fn clone(&self) -> Stream<A> {
        Stream { source: self.source.clone() }
    }    
}

impl<A: Send + Sync + Clone + 'static> Stream<A> {
    /// A stream that never fires. This can be useful in certain situations,
    /// where a stream is logically required, but no events are expected.
    pub fn never() -> Stream<A> {
        Sink::new().stream()
    }

    /// Map the stream to another stream using a function.
    ///
    /// `map` applies a function to every event fired in this stream to create a
    /// new stream of type `B`.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink: Sink<i32> = Sink::new();
    /// let mut events = sink.stream().map(|x| x + 4).events();
    /// sink.send(3);
    /// assert_eq!(events.next(), Some(7));
    /// ```
    pub fn map<B, F>(&self, f: F) -> Stream<B>
        where B: Send + Sync + Clone + 'static,
              F: Fn(A) -> B + Send + Sync + 'static,
    {
        let source = Arc::new(Mutex::new(Mapper::new(f, self.source.clone())));
        commit((), |_| {
            self.source.lock().ok().expect("Stream::map")
                .listen(source.wrap_as_listener());
            Stream { source: source.wrap_as_subject() }
        })
    }

    /// Filter a stream according to a predicate.
    ///
    /// `filter` creates a new stream that only fires those events from the
    /// original stream that satisfy the predicate.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink: Sink<i32> = Sink::new();
    /// let mut events = sink.stream()
    ///     .filter(|&x| (x >= 4) && (x <= 10))
    ///     .events();
    /// sink.send(2); // won't arrive
    /// sink.send(5); // will arrive
    /// assert_eq!(events.next(), Some(5));
    /// ```
    pub fn filter<F>(&self, f: F) -> Stream<A>
        where F: Fn(&A) -> bool + Send + Sync + 'static,
    {
        self.filter_map(move |a| if f(&a) { Some(a) } else { None })
    }

    /// Both filter and map a stream.
    ///
    /// This is equivalent to `.map(f).filter_just()`.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink = Sink::new();
    /// let mut events = sink.stream()
    ///     .filter_map(|i| if i > 3 { Some(i + 2) } else { None })
    ///     .events();
    /// sink.send(2);
    /// sink.send(4);
    /// assert_eq!(events.next(), Some(6));
    /// ```
    pub fn filter_map<B, F>(&self, f: F) -> Stream<B>
        where B: Send + Sync + Clone + 'static,
              F: Fn(A) -> Option<B> + Send + Sync + 'static,
    {
        self.map(f).filter_just()
    }

    /// Merge with another stream.
    ///
    /// `merge` takes two streams and creates a new stream that fires events
    /// from both input streams.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink_1 = Sink::<i32>::new();
    /// let sink_2 = Sink::<i32>::new();
    /// let mut events = sink_1.stream().merge(&sink_2.stream()).events();
    /// sink_1.send(2);
    /// assert_eq!(events.next(), Some(2));
    /// sink_2.send(4);
    /// assert_eq!(events.next(), Some(4));
    /// ```
    pub fn merge(&self, other: &Stream<A>) -> Stream<A> {
        let source = Arc::new(Mutex::new(Merger::new([
            self.source.clone(),
            other.source.clone(),
        ])));
        commit((), |_| {
            self.source.lock().ok().expect("Stream::merge (self)")
                .listen(source.wrap_as_listener());
            other.source.lock().ok().expect("Stream::merge (other)")
                .listen(source.wrap_as_listener());
            Stream { source: source.wrap_as_subject() }
        })
    }

    /// Hold an event in a cell.
    ///
    /// The resulting cell `hold`s the value of the last event fired by the
    /// stream.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink = Sink::new();
    /// let cell = sink.stream().hold(0);
    /// assert_eq!(cell.sample(), 0);
    /// sink.send(2);
    /// assert_eq!(cell.sample(), 2);
    /// ```
    pub fn hold(&self, initial: A) -> Cell<A> {
        let source = Arc::new(Mutex::new(
            Holder::new(initial, self.source.clone())
        ));
        commit((), |_| {
            self.source.lock().ok().expect("Stream::hold")
                .listen(source.wrap_as_listener());
            Cell { source: source.wrap_as_sampling_subject() }
        })
    }

    /// A blocking iterator over the stream.
    pub fn events(&self) -> Events<A> { Events::new(self) }

    /// Scan a stream and accumulate its event firings in a cell.
    ///
    /// Starting at some initial value, each new event changes the internal
    /// state of the resulting cell as prescribed by the supplied function.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink = Sink::new();
    /// let sum = sink.stream().scan(0, |a, b| a + b);
    /// assert_eq!(sum.sample(), 0);
    /// sink.send(2);
    /// assert_eq!(sum.sample(), 2);
    /// sink.send(4);
    /// assert_eq!(sum.sample(), 6);
    pub fn scan<B, F>(&self, initial: B, f: F) -> Cell<B>
        where B: Send + Sync + Clone + 'static,
              F: Fn(B, A) -> B + Send + Sync + 'static,
    {
        let scan = CellCycle::new(initial.clone());
        let def = scan.snapshot(self).map(move |(a, b)| f(a, b)).hold(initial);
        scan.define(def)
    }
}

impl<A: Send + Sync + Clone + 'static> Stream<Option<A>> {
    /// Filter a stream of options.
    ///
    /// `filter_just` creates a new stream that only fires the unwrapped
    /// `Some(…)` events from the original stream omitting any `None` events.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink = Sink::new();
    /// let mut events = sink.stream().filter_just().events();
    /// sink.send(None); // won't arrive
    /// sink.send(Some(5)); // will arrive
    /// assert_eq!(events.next(), Some(5));
    /// ```
    pub fn filter_just(&self) -> Stream<A> {
        let source = Arc::new(Mutex::new(Filter::new(self.source.clone())));
        commit((), |_| {
            self.source.lock().ok().expect("Stream::filter")
                .listen(source.wrap_as_listener());
            Stream { source: source.wrap_as_subject() }
        })
    }
}


/// A container of a value that changes over time.
pub struct Cell<A> {
    source: Arc<Mutex<Box<SamplingSubject<A> + 'static>>>
}

impl<A> Clone for Cell<A> {
    fn clone(&self) -> Cell<A> {
        Cell { source: self.source.clone() }
    }
}

impl<A: Send + Sync + Clone + 'static> Cell<A> {
    /// Sample the current value of the cell.
    ///
    /// `sample` provides access to the underlying data of a cell.
    pub fn sample(&self) -> A {
        commit((), |_| self.sample_nocommit())
    }

    /// Sample without committing a transaction.
    fn sample_nocommit(&self) -> A {
        self.source.lock().ok().expect("Cell::sample").sample()
    }

    /// Combine the cell with a stream in a snapshot.
    ///
    /// `snapshot` creates a new stream given a cell and a stream. Whenever the
    /// input stream fires an event, the output stream fires a pair of the
    /// cell's current value and that event.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink1: Sink<i32> = Sink::new();
    /// let sink2: Sink<f64> = Sink::new();
    /// let snapshot = sink1.stream().hold(1).snapshot(&sink2.stream());
    /// let mut events = snapshot.events();
    ///
    /// // Updating its cell does not cause the snapshot to fire
    /// sink1.send(4);
    ///
    /// // However sending an event down the stream does
    /// sink2.send(3.0);
    /// assert_eq!(events.next(), Some((4, 3.0)));
    /// ```
    pub fn snapshot<B: Send + Sync + Clone + 'static>(&self, event: &Stream<B>) -> Stream<(A, B)> {
        commit((), |_| {
            let source = Arc::new(Mutex::new(Snapper::new(
                self.sample_nocommit(), (self.source.clone(), event.source.clone())
            )));
            self.source.lock().ok().expect("Cell::snapshot (self)")
                .listen(WeakSnapperWrapper::boxed(&source));
            event.source.lock().ok().expect("Cell::snapshot (event)")
                .listen(source.wrap_as_listener());
            Stream { source: source.wrap_into_subject() }
        })
    }

    /// Creates a stream that fires updates to the cell's current value.
    pub fn updates(&self) -> Stream<A> {
        commit((), |_| {
            let source = Arc::new(Mutex::new(Updates::new(self.source.clone())));
            self.source.lock().ok().expect("Cell::updates")
                .listen(source.wrap_as_listener());
            Stream { source: source.wrap_into_subject() }
        })
    }

}

impl<A: Send + Sync + Clone + 'static> Cell<Cell<A>> {
    /// Switch between cells.
    ///
    /// This transforms a `Cell<Cell<A>>` into a `Cell<A>`. The nested cell can
    /// be thought of as a representation of a switch between different input
    /// cells, that allows one to change the structure of the dependency graph
    /// at run-time. `switch` provides a way to access the inner value of the
    /// currently active cell.
    ///
    /// The following example demonstrates how to use this to switch between two
    /// input cells based on a `Button` event stream:
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// // Button type
    /// #[derive(Clone, Show)]
    /// enum Button { A, B };
    ///
    /// // The input sinks
    /// let sink_a = Sink::<i32>::new();
    /// let sink_b = Sink::<i32>::new();
    ///
    /// // The button sink
    /// let sink_button = Sink::<Button>::new();
    ///
    /// // Create the output
    /// let output = {
    ///
    ///     // Hold input sinks in a cell with some initials
    ///     let channel_a = sink_a.stream().hold(1);
    ///     let channel_b = sink_b.stream().hold(2);
    ///
    ///     // A trivial default channel used before any button event
    ///     let default_channel = Sink::new().stream().hold(0);
    ///
    ///     // Map button to the channel cells, hold with the default channel as
    ///     // initial value and switch between the cells
    ///     sink_button
    ///         .stream()
    ///         .map(move |b| match b {
    ///             Button::A => channel_a.clone(),
    ///             Button::B => channel_b.clone(),
    ///         })
    ///         .hold(default_channel)
    ///         .switch()
    /// };
    ///
    /// // In the beginning, output will come from the default channel
    /// assert_eq!(output.sample(), 0);
    ///
    /// // Let's switch to channel A
    /// sink_button.send(Button::A);
    /// assert_eq!(output.sample(), 1);
    ///
    /// // And to channel B
    /// sink_button.send(Button::B);
    /// assert_eq!(output.sample(), 2);
    ///
    /// // The channels can change, too, of course
    /// for k in 4..13 {
    ///     sink_b.send(k);
    ///     assert_eq!(output.sample(), k);
    /// }
    /// sink_button.send(Button::A);
    /// for k in 21..77 {
    ///     sink_a.send(k);
    ///     assert_eq!(output.sample(), k);
    /// }
    /// ```
    pub fn switch(&self) -> Cell<A> {
        commit((), |_| {
            // Create the cell switcher
            let mut self_source = self.source.lock().ok().expect("Cell::switch");
            let source = CellSwitcher::new(
                self_source.sample(),
                self.source.clone(),
            );

            // Wire up
            self_source.listen(source.wrap_as_listener());

            // Create cell
            Cell { source: source.wrap_into_sampling_subject() }
        })
    }
}


/// Forward declaration of a cell to create value loops
///
/// This pattern is useful to implement accumulators, counters and other
/// loops that depend on the state of a cell before a transaction.
pub struct CellCycle<A: Send> {
    dummy: Arc<Mutex<LoopCellEntry<A>>>,
    cell: Cell<A>,
}

impl<A: Clone + Send + Sync + 'static> CellCycle<A> {
    /// Predeclare a cell
    ///
    /// FIXME: the `initial` parameter is unfortunately necessary due to a lack
    /// of laziness in the implementation.
    pub fn new(initial: A) -> CellCycle<A> {
        let dummy = Arc::new(Mutex::new(LoopCellEntry::new(initial)));
        let cell = Cell { source: dummy.wrap_as_sampling_subject() };
        CellCycle {
            dummy: dummy,
            cell: cell,
        }
    }

    /// Provide a definition to obtain a cell
    pub fn define(self, definition: Cell<A>) -> Cell<A> {
        commit((), move |_| {
            definition.source.lock().ok()
                .expect("Cell::cyclic (result listen #1)")
                .listen(self.dummy.wrap_as_listener());
            self.dummy.lock().ok()
                .expect("Cell::cyclic (result listen #3)")
                .accept(definition.sample_nocommit())
                .unwrap();
            let source = Arc::new(Mutex::new(LoopCell::new(
                definition.sample_nocommit(),
                (
                    self.dummy.wrap_into_sampling_subject(),
                    definition.source.clone(),
                )
            )));
            definition.source.lock().ok()
                .expect("Cell::cyclic (result listen #2)")
                .listen(source.wrap_as_listener());
            Cell { source: source.wrap_into_sampling_subject() }
        })
    }
}

impl<A: Send> Deref for CellCycle<A> {
    type Target = Cell<A>;
    fn deref(&self) -> &Cell<A> { &self.cell }
}


/// A blocking iterator over events in a stream.
pub struct Events<A: Send> {
    receiver: mpsc::Receiver<A>,
    #[allow(dead_code)]
    buffer: Arc<Mutex<ChannelBuffer<A>>>,
}

impl<A: Send + Sync + Clone + 'static> Events<A> {
    fn new(event: &Stream<A>) -> Events<A> {
        let (tx, rx) = mpsc::channel();
        let chanbuf = Arc::new(Mutex::new(ChannelBuffer::new(
            tx, event.source.clone()
        )));
        event.source.lock().ok().expect("Events::new")
            .listen(chanbuf.wrap_as_listener());
        Events { receiver: rx, buffer: chanbuf }
    }
}

impl<A: Send + Sync + 'static> Iterator for Events<A> {
    type Item = A;
    fn next(&mut self) -> Option<A> { self.receiver.recv().ok() }
}


#[cfg(test)]
mod test_lib;
