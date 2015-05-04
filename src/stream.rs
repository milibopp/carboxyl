//! Streams of discrete events

use std::sync::{ Arc, Mutex };
use std::sync::mpsc::{ Receiver, channel };
use std::thread;
use source::{ Source, CallbackError, with_weak };
use signal::{ self, Signal };
use transaction::commit;


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

impl<A: Send + Sync> Sink<A> {
    /// Create a new sink.
    pub fn new() -> Sink<A> {
        Sink { source: Arc::new(Mutex::new(Source::new())) }
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

    /// Send a value into the sink.
    ///
    /// When a value is sent into the sink, an event is fired in all dependent
    /// streams.
    pub fn send(&self, a: A) {
        commit((), |_| self.source.lock().unwrap().send(a))
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
pub fn source<A>(stream: &Stream<A>) -> &Arc<Mutex<Source<A>>> {
    &stream.source
}


/// A stream of discrete events.
///
/// This occasionally fires an event of type `A`.
pub struct Stream<A> {
    source: Arc<Mutex<Source<A>>>,
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
            source: Arc::new(Mutex::new(Source::new())),
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
        commit((), |_| {
            let src = Arc::new(Mutex::new(Source::new()));
            let weak = src.downgrade();
            self.source.lock().unwrap()
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
        commit((), |_| {
            let src = Arc::new(Mutex::new(Source::new()));
            for parent in [self, other].iter() {
                let weak = src.downgrade();
                parent.source.lock().unwrap()
                    .register(move |a| with_weak(&weak, |src| src.send(a)));
            }
            Stream {
                source: src,
                keep_alive: Box::new((self.clone(), other.clone())),
            }
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
    pub fn hold(&self, initial: A) -> Signal<A> {
        signal::hold(initial, self)
    }

    /// A blocking iterator over the stream.
    pub fn events(&self) -> Events<A> { Events::new(self) }
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
        commit((), |_| {
            let src = Arc::new(Mutex::new(Source::new()));
            let weak = src.downgrade();
            self.source.lock().unwrap()
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


/// Make a snapshot of a signal, whenever a stream fires an event.
pub fn snapshot<A, B>(signal: &Signal<A>, stream: &Stream<B>) -> Stream<(A, B)>
    where A: Clone + Send + Sync + 'static,
          B: Clone + Send + Sync + 'static,
{
    commit((), |_| {
        let src = Arc::new(Mutex::new(Source::new()));
        let weak = src.downgrade();
        stream.source.lock().unwrap().register({
            let signal = signal.clone();
            move |b| with_weak(&weak, |src| src.send((signal.sample(), b)))
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
        commit((), |_| {
            let (tx, rx) = channel();
            stream.source.lock().unwrap().register(
                move |a| tx.send(a).map_err(|_| CallbackError::Disappeared)
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
}
