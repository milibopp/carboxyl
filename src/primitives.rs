use std::sync::{Arc, RwLock};
use subject::{
    Subject, Source, Mapper, Receiver, WrapArc, Snapper, Merger, Filter,
    WeakSnapperWrapper, SamplingSubject, Holder, CellSwitcher, Lift2,
    WeakLift2Wrapper
};


pub struct Sink<A> {
    source: Arc<RwLock<Source<A>>>,
}

impl<A> Clone for Sink<A> {
    fn clone(&self) -> Sink<A> {
        Sink { source: self.source.clone() }
    }    
}

impl<A: Send + Sync + Clone> Sink<A> {
    pub fn new() -> Sink<A> {
        Sink { source: Arc::new(RwLock::new(Source::new())) }
    }

    pub fn send(&self, a: A) {
        self.source.write().unwrap().send(a)
    }

    pub fn event(&self) -> Stream<A> {
        Stream { source: self.source.wrap_as_subject() }
    }
}


pub struct Stream<A> {
    source: Arc<RwLock<Box<Subject<A> + 'static>>>,
}

impl<A> Clone for Stream<A> {
    fn clone(&self) -> Stream<A> {
        Stream { source: self.source.clone() }
    }    
}

impl<A: Send + Sync + Clone> Stream<A> {
    pub fn map<B, F>(&self, f: F) -> Stream<B>
        where B: Send + Sync + Clone,
              F: Fn(A) -> B + Send + Sync,
    {
        let source = Arc::new(RwLock::new(Mapper::new(f, self.source.clone())));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        Stream { source: source.wrap_into_subject() }
    }

    pub fn filter<F: Fn(&A) -> bool + Send + Sync>(&self, f: F) -> Stream<A> {
        let source = Arc::new(RwLock::new(Filter::new(f, self.source.clone())));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        Stream { source: source.wrap_into_subject() }
    }

    pub fn merge(&self, other: &Stream<A>) -> Stream<A> {
        let source = Arc::new(RwLock::new(Merger::new([
            self.source.clone(),
            other.source.clone(),
        ])));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        other.source.write().unwrap().listen(source.wrap_as_listener());
        Stream { source: source.wrap_into_subject() }
    }

    pub fn hold(&self, initial: A) -> Cell<A> {
        let source = Arc::new(RwLock::new(
            Holder::new(initial, self.source.clone())
        ));
        self.source.write().unwrap().listen(source.wrap_as_listener());
        Cell { source: source.wrap_into_sampling_subject() }
    }

    fn iter(&self) -> Iter<A> { Iter::new(self) }
}


pub struct Cell<A> {
    source: Arc<RwLock<Box<SamplingSubject<A> + 'static>>>
}

impl<A> Clone for Cell<A> {
    fn clone(&self) -> Cell<A> {
        Cell { source: self.source.clone() }
    }
}

impl<A: Send + Sync + Clone> Cell<A> {
    pub fn sample(&self) -> A {
        self.source.write().unwrap().sample()
    }

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
    //use behaviour::Cell;
    use super::*;

    #[test]
    fn sink() {
        let sink = Sink::new();
        let mut iter = sink.event().iter();
        sink.send(1);
        sink.send(2);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
    }

    #[test]
    fn map() {
        let sink = Sink::new();
        let triple = sink.event().map(|x| 3 * x);
        let mut iter = triple.iter();
        sink.send(1);
        assert_eq!(iter.next(), Some(3));
    }

    #[test]
    fn filter() {
        let sink = Sink::new();
        let small = sink.event().filter(|&x: &i32| x < 11);
        let mut iter = small.iter();
        sink.send(12);
        sink.send(9);
        assert_eq!(iter.next(), Some(9));
    }

    #[test]
    fn merge() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let mut iter = sink1.event().merge(&sink2.event()).iter();
        sink1.send(12);
        sink2.send(9);
        assert_eq!(iter.next(), Some(12));
        assert_eq!(iter.next(), Some(9));
    }

    #[test]
    fn hold() {
        let ea = Sink::new();
        let ba = ea.event().hold(3);
        assert_eq!(ba.sample(), 3);
        ea.send(4);
        assert_eq!(ba.sample(), 4);
    }

    #[test]
    fn chain_1() {
        let sink = Sink::new();
        let chain = sink.event().filter(|&x: &i32| x < 20).map(|x| x / 2);
        let mut iter = chain.iter();
        sink.send(4);
        assert_eq!(iter.next(), Some(2));
    }

    #[test]
    fn chain_2() {
        let sink: Sink<i32> = Sink::new();
        let chain = sink.event().map(|x| x / 2).filter(|&x| x < 3);
        let mut iter = chain.iter();
        sink.send(4);
        assert_eq!(iter.next(), Some(2));
    }

    #[test]
    fn chain_3() {
        let sink1: Sink<i32> = Sink::new();
        let sink2: Sink<i32> = Sink::new();
        let mut iter = sink1.event().map(|x| x + 4)
            .merge(&sink2.event().filter(|&x| x < 4).map(|x| x * 5))
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
        let mut snap_iter = sink1.event().hold(1).snapshot(&sink2.event().map(|x| x + 3.0)).iter();
        sink2.send(4.0);
        assert_eq!(snap_iter.next(), Some((1, 7.0)));
    }

    #[test]
    fn snapshot_2() {
        let ev1 = Sink::new();
        let beh1 = ev1.event().hold(5);
        let ev2 = Sink::new();
        let snap = beh1.snapshot(&ev2.event());
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
        let lifted = lift2(|a, b| a + b, &sink1.event().hold(0), &sink2.event().hold(3));
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
        let tv = {
            let stream1 = stream1.event().hold(Some(-1));
            let stream2 = stream2.event().hold(Some(-2));
            button1.event()
                .map(move |_| { println!("switch to 1"); stream1.clone() })
                .merge(&button2.event()
                    .map(move |_| stream2.clone())
                )
                .hold(Sink::new().event().hold(Some(-3)))
                .switch()
        };
        assert_eq!(tv.sample(), Some(-3));
        button1.send(());
        assert_eq!(tv.sample(), Some(-1));
        button2.send(());
        assert_eq!(tv.sample(), Some(-2));
        stream1.send(Some(6));
        button1.send(());
        assert_eq!(tv.sample(), Some(6));
    }

    #[test]
    fn clone() {
        let sink = Sink::new();
        let b = sink.event().hold(1);
        sink.clone().send(3);
        assert_eq!(b.sample(), 3);
    }
}
