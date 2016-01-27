//! Streams of discrete events

use std::sync::{ Arc, RwLock, Mutex, Weak };
use std::sync::mpsc::{ Receiver, channel };
use std::thread;
use source::{ Source, CallbackError, CallbackResult, with_weak };
use signal::{ self, Signal, SignalMut, sample_raw };
use transaction::commit;


mod coalesce;


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
    source: Arc<RwLock<Source<A>>>,
}

impl<A> Clone for Sink<A> {
    fn clone(&self) -> Sink<A> {
        Sink { source: self.source.clone() }
    }
}

impl<A: Send + Sync> Sink<A> {
    /// Create a new sink.
    pub fn new() -> Sink<A> {
        Sink { source: Arc::new(RwLock::new(Source::new())) }
    }

    /// Generate a stream that fires all events sent into the sink.
    pub fn stream(&self) -> Stream<A> {
        Stream { source: self.source.clone(), keep_alive: Box::new(()), }
    }
}

impl<A: Send + Sync + Clone + 'static> Sink<A> {
    /// Asynchronous send.
    ///
    /// Same as `send`, but it spawns a new thread to process the updates to
    /// dependent streams and signals.
    pub fn send_async(&self, a: A) {
        let clone = self.clone();
        thread::spawn(move || clone.send(a));
    }

    /// Feed values from an iterator into the sink.
    ///
    /// This method feeds events into the sink from an iterator.
    pub fn feed<I: IntoIterator<Item=A>>(&self, iterator: I) {
        for event in iterator {
            self.send(event);
        }
    }

    /// Asynchronous feed.
    ///
    /// This is the same as `feed`, but it does not block, since it spawns the
    /// feeding as a new task. This is useful, if the provided iterator is large
    /// or even infinite (e.g. an I/O event loop).
    pub fn feed_async<I: IntoIterator<Item=A> + Send + 'static>(&self, iterator: I) {
        let clone = self.clone();
        thread::spawn(move || clone.feed(iterator));
    }

    /// Send a value into the sink.
    ///
    /// When a value is sent into the sink, an event is fired in all dependent
    /// streams.
    pub fn send(&self, a: A) {
        commit(|| self.source.write().unwrap().send(a))
    }
}


/// Trait to wrap cloning of boxed values in a object-safe manner
pub trait BoxClone: Sync + Send {
    /// Clone the object as a boxed trait object
    fn box_clone(&self) -> Box<BoxClone>; 
}

impl<T: Sync + Send + Clone + 'static> BoxClone for T {
    fn box_clone(&self) -> Box<BoxClone> {
        Box::new(self.clone())
    }
}


/// Access a stream's source.
///
/// This is not defined as a method, so that it can be public to other modules
/// in this crate while being private outside the crate.
pub fn source<A>(stream: &Stream<A>) -> &Arc<RwLock<Source<A>>> {
    &stream.source
}


/// A stream of events.
///
/// Conceptually a stream can be thought of as a series of discrete events that
/// occur at specific times. They are ordered by a transaction system. This
/// means that firings of disjoint events can not interfere with each other. The
/// consequences of one event are atomically reflected in dependent quantities.
///
/// Streams provide a number of primitive operations. These can be used to
/// compose streams and combine them with signals. For instance, streams can be
/// mapped over with a function, merged with another stream of the same type or
/// filtered by some predicate.
///
/// # Algebraic laws
///
/// Furthermore, streams satisfy certain algebraic properties that are useful to
/// reason about them.
///
/// ## Monoid
///
/// For once, streams of the same type form a **monoid** under merging. The
/// neutral element in this context is `Stream::never()`. So the following laws
/// always hold for streams `a`, `b` and `c` of the same type:
///
/// - Left identity: `Stream::never().merge(&a) == a`,
/// - Right identity: `a.merge(&Stream::never()) == a`,
/// - Associativity: `a.merge(&b).merge(&c) == a.merge(&b.merge(&c))`.
///
/// *Note that equality in this context is not actually implemented as such,
/// since comparing two (potentially infinite) streams is a prohibitive
/// operation. Instead, the expressions above can be used interchangably and
/// behave identically.*
///
/// ## Functor
///
/// Under the mapping operation streams also become a functor. A functor is a
/// generic type like `Stream` with some mapping operation that takes a function
/// `Fn(A) -> B` to map a `Stream<A>` to a `Stream<B>`. Algebraically it
/// satisfies the following laws:
///
/// - The identity function is preserved: `a.map(|x| x) == a`,
/// - Function composition is respected: `a.map(f).map(g) == a.map(|x| g(f(x)))`.
pub struct Stream<A> {
    source: Arc<RwLock<Source<A>>>,
    #[allow(dead_code)]
    keep_alive: Box<BoxClone>,
}

impl<A> Clone for Stream<A> {
    fn clone(&self) -> Stream<A> {
        Stream {
            source: self.source.clone(),
            keep_alive: self.keep_alive.box_clone(),
        }
    }
}


impl<A: Clone + Send + Sync + 'static> Stream<A> {
    /// Create a stream that never fires. This can be useful in certain
    /// situations, where a stream is logically required, but no events are
    /// expected.
    pub fn never() -> Stream<A> {
        Stream {
            source: Arc::new(RwLock::new(Source::new())),
            keep_alive: Box::new(()) 
        }
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
        commit(|| {
            let src = Arc::new(RwLock::new(Source::new()));
            let weak = Arc::downgrade(&src);
            self.source.write().unwrap()
                .register(move |a| with_weak(&weak, |src| src.send(f(a))));
            Stream {
                source: src,
                keep_alive: Box::new(self.clone()),
            }
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
    /// This is equivalent to `.map(f).filter_some()`.
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
        self.map(f).filter_some()
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
        commit(|| {
            let src = Arc::new(RwLock::new(Source::new()));
            for parent in [self, other].iter() {
                let weak = Arc::downgrade(&src);
                parent.source.write().unwrap()
                    .register(move |a| with_weak(&weak, |src| src.send(a)));
            }
            Stream {
                source: src,
                keep_alive: Box::new((self.clone(), other.clone())),
            }
        })
    }

    /// Coalesce multiple event firings within the same transaction into a
    /// single event.
    ///
    /// The function should ideally commute, as the order of events within a
    /// transaction is not well-defined.
    pub fn coalesce<F>(&self, reducer: F) -> Stream<A>
        where F: Fn(A, A) -> A + Send + Sync + 'static,
    {
        commit(|| coalesce::stream(self, reducer))
    }

    /// Hold an event in a signal.
    ///
    /// The resulting signal `hold`s the value of the last event fired by the
    /// stream.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink = Sink::new();
    /// let signal = sink.stream().hold(0);
    /// assert_eq!(signal.sample(), 0);
    /// sink.send(2);
    /// assert_eq!(signal.sample(), 2);
    /// ```
    pub fn hold(&self, initial: A) -> Signal<A> {
        signal::hold(initial, self)
    }

    /// A blocking iterator over the stream.
    pub fn events(&self) -> Events<A> { Events::new(self) }

    /// Scan a stream and accumulate its event firings in a signal.
    ///
    /// Starting at some initial value, each new event changes the value of the
    /// resulting signal as prescribed by the supplied function.
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
    /// ```
    pub fn scan<B, F>(&self, initial: B, f: F) -> Signal<B>
        where B: Send + Sync + Clone + 'static,
              F: Fn(B, A) -> B + Send + Sync + 'static,
    {
        Signal::cyclic(|scan| scan.snapshot(self, f).hold(initial))
    }

    /// Scan a stream and accumulate its event firings in some mutable state.
    ///
    /// Semantically this is equivalent to `scan`. However, it allows one to use
    /// a non-Clone type as an accumulator and update it with efficient in-place
    /// operations.
    ///
    /// The resulting `SignalMut` does have a slightly different API from a
    /// regular `Signal` as it does not allow clones.
    ///
    /// # Example
    ///
    /// ```
    /// # use carboxyl::{ Sink, Signal };
    /// let sink: Sink<i32> = Sink::new();
    /// let sum = sink.stream()
    ///     .scan_mut(0, |sum, a| *sum += a)
    ///     .combine(&Signal::new(()), |sum, ()| *sum);
    /// assert_eq!(sum.sample(), 0);
    /// sink.send(2);
    /// assert_eq!(sum.sample(), 2);
    /// sink.send(4);
    /// assert_eq!(sum.sample(), 6);
    /// ```
    pub fn scan_mut<B, F>(&self, initial: B, f: F) -> SignalMut<B>
        where B: Send + Sync + 'static,
              F: Fn(&mut B, A) + Send + Sync + 'static,
    {
        signal::scan_mut(self, initial, f)
    }
}

impl<A: Clone + Send + Sync + 'static> Stream<Option<A>> {
    /// Filter a stream of options.
    ///
    /// `filter_some` creates a new stream that only fires the unwrapped
    /// `Some(…)` events from the original stream omitting any `None` events.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink = Sink::new();
    /// let mut events = sink.stream().filter_some().events();
    /// sink.send(None); // won't arrive
    /// sink.send(Some(5)); // will arrive
    /// assert_eq!(events.next(), Some(5));
    /// ```
    pub fn filter_some(&self) -> Stream<A> {
        commit(|| {
            let src = Arc::new(RwLock::new(Source::new()));
            let weak = Arc::downgrade(&src);
            self.source.write().unwrap()
                .register(move |a| a.map_or(
                    Ok(()),
                    |a| with_weak(&weak, |src| src.send(a))
                ));
            Stream {
                source: src,
                keep_alive: Box::new(self.clone())
            }
        })
    }
}

impl<A: Send + Sync + Clone + 'static> Stream<Stream<A>> {
    /// Switch between streams.
    ///
    /// This takes a stream of streams and maps it to a new stream, which fires
    /// all events from the most recent stream fired into it.
    ///
    /// # Example
    ///
    /// ```
    /// # use carboxyl::{ Sink, Stream };
    /// // Create sinks
    /// let stream_sink: Sink<Stream<i32>> = Sink::new();
    /// let sink1: Sink<i32> = Sink::new();
    /// let sink2: Sink<i32> = Sink::new();
    ///
    /// // Switch and listen
    /// let switched = stream_sink.stream().switch();
    /// let mut events = switched.events();
    ///
    /// // Should not receive events from either sink
    /// sink1.send(1); sink2.send(2);
    ///
    /// // Now switch to sink 2
    /// stream_sink.send(sink2.stream());
    /// sink1.send(3); sink2.send(4);
    /// assert_eq!(events.next(), Some(4));
    ///
    /// // And then to sink 1
    /// stream_sink.send(sink1.stream());
    /// sink1.send(5); sink2.send(6);
    /// assert_eq!(events.next(), Some(5));
    /// ```
    pub fn switch(&self) -> Stream<A> {
        fn rewire_callbacks<A>(new_stream: Stream<A>, source: Weak<RwLock<Source<A>>>,
                               terminate: &mut Arc<()>)
            -> CallbackResult
            where A: Send + Sync + Clone + 'static,
        {
            *terminate = Arc::new(());
            let weak = Arc::downgrade(&terminate);
            new_stream.source.write().unwrap().register(move |a|
                weak.upgrade()
                    .ok_or(CallbackError::Disappeared)
                    .and_then(|_| with_weak(&source, |src| src.send(a)))
            );
            Ok(())
        }
        commit(|| {
            let src = Arc::new(RwLock::new(Source::new()));
            let weak = Arc::downgrade(&src);
            self.source.write().unwrap().register({
                let mut terminate = Arc::new(());
                move |stream| rewire_callbacks(stream, weak.clone(), &mut terminate)
            });
            Stream {
                source: src,
                keep_alive: Box::new(self.clone()),
            }
        })
    }
}


/// Make a snapshot of a signal, whenever a stream fires an event.
pub fn snapshot<A, B, C, F>(signal: &Signal<A>, stream: &Stream<B>, f: F) -> Stream<C>
    where A: Clone + Send + Sync + 'static,
          B: Clone + Send + Sync + 'static,
          C: Clone + Send + Sync + 'static,
          F: Fn(A, B) -> C + Send + Sync + 'static,
{
    commit(|| {
        let src = Arc::new(RwLock::new(Source::new()));
        let weak = Arc::downgrade(&src);
        stream.source.write().unwrap().register({
            let signal = signal.clone();
            move |b| with_weak(&weak, |src| src.send(f(sample_raw(&signal), b)))
        });
        Stream {
            source: src,
            keep_alive: Box::new((stream.clone(), signal.clone())),
        }
    })
}


/// A blocking iterator over events in a stream.
pub struct Events<A> {
    receiver: Receiver<A>,
    #[allow(dead_code)]
    keep_alive: Box<BoxClone>,
}

impl<A: Send + Sync + 'static> Events<A> {
    /// Create a new events iterator.
    fn new(stream: &Stream<A>) -> Events<A> {
        commit(|| {
            let (tx, rx) = channel();
            let tx = Mutex::new(tx);
            stream.source.write().unwrap().register(
                move |a| tx.lock().unwrap().send(a).map_err(|_| CallbackError::Disappeared)
            );
            Events {
                receiver: rx,
                keep_alive: Box::new(stream.clone()),
            }
        })
    }
}

impl<A: Send + Sync + 'static> Iterator for Events<A> {
    type Item = A;
    fn next(&mut self) -> Option<A> { self.receiver.recv().ok() }
}


#[cfg(test)]
mod test {
    use std::thread;
    use std::time::Duration;
    use quickcheck::quickcheck;

    use testing::{ id, stream_eq };
    use super::*;

    #[test]
    fn sink() {
        let sink = Sink::new();
        let mut events = sink.stream().events();
        sink.send(1);
        sink.send(2);
        assert_eq!(events.next(), Some(1));
        assert_eq!(events.next(), Some(2));
    }

    #[test]
    fn map() {
        let sink = Sink::new();
        let triple = sink.stream().map(|x| 3 * x);
        let mut events = triple.events();
        sink.send(1);
        assert_eq!(events.next(), Some(3));
    }

    #[test]
    fn filter_some() {
        let sink = Sink::new();
        let small = sink.stream().filter_some();
        let mut events = small.events();
        sink.send(None);
        sink.send(Some(9));
        assert_eq!(events.next(), Some(9));
    }

    #[test]
    fn chain_1() {
        let sink: Sink<i32> = Sink::new();
        let chain = sink.stream()
            .map(|x| x / 2)
            .filter(|&x| x < 3);
        let mut events = chain.events();
        sink.send(7);
        sink.send(4);
        assert_eq!(events.next(), Some(2));
    }

    #[test]
    fn merge() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let mut events = sink1.stream().merge(&sink2.stream()).events();
        sink1.send(12);
        sink2.send(9);
        assert_eq!(events.next(), Some(12));
        assert_eq!(events.next(), Some(9));
    }

    #[test]
    fn chain_2() {
        let sink1: Sink<i32> = Sink::new();
        let sink2: Sink<i32> = Sink::new();
        let mut events = sink1.stream().map(|x| x + 4)
            .merge(
                &sink2.stream()
                .filter_map(|x| if x < 4 { Some(x) } else { None })
                .map(|x| x * 5))
            .events();
        sink1.send(12);
        sink2.send(3);
        assert_eq!(events.next(), Some(16));
        assert_eq!(events.next(), Some(15));
    }

    #[test]
    fn move_closure() {
        let sink = Sink::<i32>::new();
        let x = 3;
        sink.stream().map(move |y| y + x);
    }

    #[test]
    fn scan_race_condition() {
        let sink = Sink::new();
        // Feed the sink in the background
        sink.feed_async(0..100000);
        // Try it multiple times to increase failure probability, when a data
        // race can potentially happen.
        for _ in 0..10 {
            let _sum = sink.stream().scan(0, |a, b| a + b);
        }
    }

    #[test]
    fn sink_send_async() {
        let sink = Sink::new();
        let mut events = sink.stream().events();
        sink.send_async(1);
        assert_eq!(events.next(), Some(1));
    }

    #[test]
    fn sink_feed() {
        let sink = Sink::new();
        let events = sink.stream().events();
        sink.feed(0..10);
        for (n, m) in events.take(10).enumerate() {
            assert_eq!(n as i32, m);
        }
    }

    #[test]
    fn sink_feed_async() {
        let sink = Sink::new();
        let events = sink.stream().events();
        sink.feed_async(0..10);
        for (n, m) in events.take(10).enumerate() {
            assert_eq!(n as i32, m);
        }
    }

    #[test]
    fn coalesce() {
        let sink = Sink::new();
        let stream = sink.stream()
            .merge(&sink.stream())
            .coalesce(|a, b| a + b);
        let mut events = stream.events();

        sink.send(1);
        assert_eq!(events.next(), Some(2));
    }

    #[test]
    fn monoid_left_identity() {
        fn check(input: Vec<i32>) -> Result<(), String> {
            let sink = Sink::new();
            let a = sink.stream();
            let eq = stream_eq(&Stream::never().merge(&a), &a);
            sink.feed(input.into_iter());
            eq.sample()
        }
        quickcheck(check as fn(Vec<i32>) -> Result<(), String>);
    }

    #[test]
    fn monoid_right_identity() {
        fn check(input: Vec<i32>) -> Result<(), String> {
            let sink = Sink::new();
            let a = sink.stream();
            let eq = stream_eq(&a.merge(&Stream::never()), &a);
            sink.feed(input.into_iter());
            eq.sample()
        }
        quickcheck(check as fn(Vec<i32>) -> Result<(), String>);
    }

    #[test]
    fn monoid_associative() {
        fn check(input_a: Vec<i32>, input_b: Vec<i32>, input_c: Vec<i32>) -> Result<(), String> {
            let sink_a = Sink::new();
            let sink_b = Sink::new();
            let sink_c = Sink::new();
            let a = sink_a.stream();
            let b = sink_b.stream();
            let c = sink_c.stream();
            let eq = stream_eq(&a.merge(&b.merge(&c)), &a.merge(&b).merge(&c));
            // TODO: replace this spawn & wait mechanism by scoped threads
            thread::spawn(move || sink_a.feed(input_a.into_iter()));
            thread::spawn(move || sink_b.feed(input_b.into_iter()));
            thread::spawn(move || sink_c.feed(input_c.into_iter()));
            thread::sleep(Duration::from_millis(1));
            eq.sample()
        }
        quickcheck(check as fn(Vec<i32>, Vec<i32>, Vec<i32>) -> Result<(), String>);
    }

    #[test]
    fn functor_identity() {
        fn check(input: Vec<i32>) -> Result<(), String> {
            let sink = Sink::new();
            let a = sink.stream();
            let eq = stream_eq(&a.map(id), &a);
            sink.feed(input.into_iter());
            eq.sample()
        }
        quickcheck(check as fn(Vec<i32>) -> Result<(), String>);
    }

    #[test]
    fn functor_composition() {
        fn check(input: Vec<i32>) -> Result<(), String> {
            fn f(n: i32) -> i64 { (n + 3) as i64 }
            fn g(n: i64) -> f64 { n as f64 / 2.5 }

            let sink = Sink::new();
            let a = sink.stream();
            let eq = stream_eq(&a.map(f).map(g), &a.map(|n| g(f(n))));
            sink.feed(input.into_iter());
            eq.sample()
        }
        quickcheck(check as fn(Vec<i32>) -> Result<(), String>);
    }
}
