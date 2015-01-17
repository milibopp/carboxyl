use std::sync::{Arc, RwLock};
use subject::{
    Subject, Source, Mapper, Receiver, WrapArc, Snapper, Merger, Filter,
    WeakSnapperWrapper, SamplingSubject, Holder, CellSwitcher, Lift2,
    WeakLift2Wrapper
};


/// An event sink
///
/// This primitive is the canonical way of generating streams of events. One can
/// send values into a sink and generate a stream that fires all these values as
/// events.
pub struct Sink<A> {
    source: Arc<RwLock<Source<A>>>,
}

impl<A> Clone for Sink<A> {
    fn clone(&self) -> Sink<A> {
        Sink { source: self.source.clone() }
    }    
}

impl<A: Send + Sync + Clone> Sink<A> {
    /// Create a new sink
    pub fn new() -> Sink<A> {
        Sink { source: Arc::new(RwLock::new(Source::new())) }
    }

    /// Send a value into the sink, firing an event in all dependent streams
    pub fn send(&self, a: A) {
        self.source.write().unwrap().send(a)
    }

    /// Generate a stream that fires all events sent into the sink
    pub fn stream(&self) -> Stream<A> {
        Stream { source: self.source.wrap_as_subject() }
    }
}


/// A stream of discrete events of type `A`
///
/// This occasionally fires an event of type `A` down its dependency graph.
pub struct Stream<A> {
    source: Arc<RwLock<Box<Subject<A> + 'static>>>,
}

impl<A> Clone for Stream<A> {
    fn clone(&self) -> Stream<A> {
        Stream { source: self.source.clone() }
    }    
}

impl<A: Send + Sync + Clone> Stream<A> {
    /// Map the stream to another stream using a function
    ///
    /// `map` applies a function to every event fired in this stream to create a
    /// new stream of type `B`.
    pub fn map<B, F>(&self, f: F) -> Stream<B>
        where B: Send + Sync + Clone,
              F: Fn(A) -> B + Send + Sync,
    {
        let source = Arc::new(RwLock::new(Mapper::new(f, self.source.clone())));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        Stream { source: source.wrap_into_subject() }
    }

    /// Filter the stream using a predicate
    ///
    /// `filter` creates a new stream that evaluates a predicate to generate a
    /// new stream of events. The resulting stream only fires those events from
    /// the original stream that satisfy the predicate.
    pub fn filter<F: Fn(&A) -> bool + Send + Sync>(&self, f: F) -> Stream<A> {
        let source = Arc::new(RwLock::new(Filter::new(f, self.source.clone())));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        Stream { source: source.wrap_into_subject() }
    }

    /// Merge with another stream
    ///
    /// `merge` takes two streams and creates a new stream that fires events
    /// from both input streams.
    pub fn merge(&self, other: &Stream<A>) -> Stream<A> {
        let source = Arc::new(RwLock::new(Merger::new([
            self.source.clone(),
            other.source.clone(),
        ])));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        other.source.write().unwrap().listen(source.wrap_as_listener());
        Stream { source: source.wrap_into_subject() }
    }

    /// Hold an event in a cell
    ///
    /// The resulting cell `hold`s the value of the last event fired by the
    /// stream.
    pub fn hold(&self, initial: A) -> Cell<A> {
        let source = Arc::new(RwLock::new(
            Holder::new(initial, self.source.clone())
        ));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        Cell { source: source.wrap_into_sampling_subject() }
    }

    fn iter(&self) -> Iter<A> { Iter::new(self) }
}


/// A `Cell` is an abstraction over a value that changes over time
pub struct Cell<A> {
    source: Arc<RwLock<Box<SamplingSubject<A> + 'static>>>
}

impl<A> Clone for Cell<A> {
    fn clone(&self) -> Cell<A> {
        Cell { source: self.source.clone() }
    }
}

impl<A: Send + Sync + Clone> Cell<A> {
    /// Sample the current value of the cell
    ///
    /// `sample` provides access to the underlying data of a cell.
    pub fn sample(&self) -> A {
        self.source.write().unwrap().sample()
    }

    /// Combine the cell with a stream in a snapshot
    ///
    /// `snapshot` creates a new stream given a cell and a stream. Whenever the
    /// input stream fires an event, the output stream fires a pair of the
    /// cell's current value and that event.
    pub fn snapshot<B: Send + Sync + Clone>(&self, event: &Stream<B>) -> Stream<(A, B)> {
        let source = Arc::new(RwLock::new(Snapper::new(
            self.sample(), (self.source.clone(), event.source.clone())
        )));
        self.source.write().unwrap()
            .listen(WeakSnapperWrapper::boxed(&source));
        event.source.write().unwrap().listen(source.wrap_as_listener());
        Stream { source: source.wrap_into_subject() }
    }
}

impl<A: Send + Sync + Clone> Cell<Cell<A>> {
    /// Switch between cells
    ///
    /// This transforms a `Cell<Cell<A>>` into a `Cell<A>`. The nested cell can
    /// be thought of as a representation of a switch between different input
    /// cells, that allows one to change the structure of the dependency graph
    /// at run-time. `switch` provides a way to access the inner value of the
    /// currently active cell.
    pub fn switch(&self) -> Cell<A> {
        let source = Arc::new(RwLock::new(
            CellSwitcher::new(
                self.sample().sample(),
                self.source.clone(),
            )
        ));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        Cell { source: source.wrap_into_sampling_subject() }
    }
}


/// Lift a two-argument function
///
/// A lift maps a function on values to a function on cells. This particular
/// function works only with a two-argument function and effectively turns two
/// cells over types `A` and `B` into a cell over type `C`, given a function of
/// type `F: Fn(A, B) -> C` by simply applying it two the inner values of the
/// cells. So essentially `f(ba.sample(), bb.sample())` is the same as
/// `lift2(f, ba, bb).sample()`.
pub fn lift2<A, B, C, F>(f: F, ba: &Cell<A>, bb: &Cell<B>) -> Cell<C>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          C: Send + Sync + Clone,
          F: Fn(A, B) -> C + Send + Sync,
{
    let source = Arc::new(RwLock::new(Lift2::new(
        (ba.sample(), bb.sample()), f, (ba.source.clone(), bb.source.clone())
    )));
    ba.source.write().unwrap().listen(source.wrap_as_listener());
    bb.source.write().unwrap().listen(WeakLift2Wrapper::boxed(&source));
    Cell { source: source.wrap_into_sampling_subject() }
}


struct Iter<A> {
    recv: Arc<RwLock<Receiver<A>>>,
}

impl<A> Clone for Iter<A> {
    fn clone(&self) -> Iter<A> {
        Iter { recv: self.recv.clone() }
    }
}

impl<A: Send + Sync + Clone> Iter<A> {
    fn new(event: &Stream<A>) -> Iter<A> {
        let iter = Iter {
            recv: Arc::new(RwLock::new(Receiver::new(
                event.source.clone()
            ))),
        };
        event.source.write().unwrap().listen(iter.recv.wrap_as_listener());
        iter
    }
}

impl<A: Send + Sync> Iterator for Iter<A> {
    type Item = A;
    fn next(&mut self) -> Option<A> {
        self.recv.write().unwrap().next()
    }
}


#[cfg(test)]
mod test {
    use test::Bencher;
    use super::*;

    #[test]
    fn sink() {
        let sink = Sink::new();
        let mut iter = sink.stream().iter();
        sink.send(1);
        sink.send(2);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
    }

    #[test]
    fn map() {
        let sink = Sink::new();
        let triple = sink.stream().map(|x| 3 * x);
        let mut iter = triple.iter();
        sink.send(1);
        assert_eq!(iter.next(), Some(3));
    }

    #[test]
    fn filter() {
        let sink = Sink::new();
        let small = sink.stream().filter(|&x: &i32| x < 11);
        let mut iter = small.iter();
        sink.send(12);
        sink.send(9);
        assert_eq!(iter.next(), Some(9));
    }

    #[test]
    fn merge() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let mut iter = sink1.stream().merge(&sink2.stream()).iter();
        sink1.send(12);
        sink2.send(9);
        assert_eq!(iter.next(), Some(12));
        assert_eq!(iter.next(), Some(9));
    }

    #[test]
    fn hold() {
        let ea = Sink::new();
        let ba = ea.stream().hold(3);
        assert_eq!(ba.sample(), 3);
        ea.send(4);
        assert_eq!(ba.sample(), 4);
    }

    #[test]
    fn chain_1() {
        let sink = Sink::new();
        let chain = sink.stream().filter(|&x: &i32| x < 20).map(|x| x / 2);
        let mut iter = chain.iter();
        sink.send(4);
        assert_eq!(iter.next(), Some(2));
    }

    #[test]
    fn chain_2() {
        let sink: Sink<i32> = Sink::new();
        let chain = sink.stream().map(|x| x / 2).filter(|&x| x < 3);
        let mut iter = chain.iter();
        sink.send(4);
        assert_eq!(iter.next(), Some(2));
    }

    #[test]
    fn chain_3() {
        let sink1: Sink<i32> = Sink::new();
        let sink2: Sink<i32> = Sink::new();
        let mut iter = sink1.stream().map(|x| x + 4)
            .merge(&sink2.stream().filter(|&x| x < 4).map(|x| x * 5))
            .iter();
        sink1.send(12);
        sink2.send(3);
        assert_eq!(iter.next(), Some(16));
        assert_eq!(iter.next(), Some(15));
    }

    #[test]
    fn snapshot() {
        let sink1: Sink<i32> = Sink::new();
        let sink2: Sink<f64> = Sink::new();
        let mut snap_iter = sink1.stream().hold(1).snapshot(&sink2.stream().map(|x| x + 3.0)).iter();
        sink2.send(4.0);
        assert_eq!(snap_iter.next(), Some((1, 7.0)));
    }

    #[test]
    fn snapshot_2() {
        let ev1 = Sink::new();
        let beh1 = ev1.stream().hold(5);
        let ev2 = Sink::new();
        let snap = beh1.snapshot(&ev2.stream());
        let mut iter = snap.iter();
        assert_eq!(iter.next(), None);
        ev2.send(4);
        assert_eq!(iter.next(), Some((5, 4)));
        ev1.send(-2);
        assert_eq!(iter.next(), None);
        ev2.send(6);
        assert_eq!(iter.next(), Some((-2, 6)));
    }

    #[test]
    fn lift2_test() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let lifted = lift2(|a, b| a + b, &sink1.stream().hold(0), &sink2.stream().hold(3));
        assert_eq!(lifted.sample(), 3);
        sink1.send(1);
        assert_eq!(lifted.sample(), 4);
        sink2.send(11);
        assert_eq!(lifted.sample(), 12);
    }

    #[test]
    fn switch() {
        let stream1 = Sink::<Option<i32>>::new();
        let stream2 = Sink::<Option<i32>>::new();
        let button1 = Sink::<()>::new();
        let button2 = Sink::<()>::new();
        let output = {
            let stream1 = stream1.stream().hold(Some(-1));
            let stream2 = stream2.stream().hold(Some(-2));
            button1.stream()
                .map(move |_| stream1.clone())
                .merge(&button2.stream()
                    .map(move |_| stream2.clone())
                )
                .hold(Sink::new().stream().hold(Some(-3)))
                .switch()
        };
        assert_eq!(output.sample(), Some(-3));
        button1.send(());
        assert_eq!(output.sample(), Some(-1));
        button2.send(());
        assert_eq!(output.sample(), Some(-2));
        stream1.send(Some(6));
        button1.send(());
        assert_eq!(output.sample(), Some(6));
    }

    #[test]
    fn clone() {
        let sink = Sink::new();
        let b = sink.stream().hold(1);
        sink.clone().send(3);
        assert_eq!(b.sample(), 3);
    }

    #[test]
    fn move_closure() {
        let sink = Sink::<i32>::new();
        let x = 3;
        sink.stream().map(move |y| y + x);
    }
}
