use std::sync::{Arc, RwLock};
use subject::{
    self, Subject, Source, Mapper, Receiver, WrapListener, Snapper,
    SnapperWrapper,
};
use behaviour::{Behaviour, Hold};


pub trait HasSource<A> {
    type Source: Subject<A>;

    fn source(&self) -> &Arc<RwLock<Self::Source<A>>>;
}

pub trait Event<A: Send + Sync + Clone>: HasSource<A> + Sized {
    fn map<B, F>(&self, f: F) -> Map<A, B, F>
        where B: Send + Sync + Clone,
              F: Fn(A) -> B + Send + Sync,
    {
        Map::new(self, f)
    }

    fn filter<F>(&self, f: F) -> Filter<A, F>
        where F: Fn(&A) -> bool + Send + Sync,
    {
        Filter::new(self, f)
    }

    fn merge<E: Event<A>>(&self, other: &E) -> Merge<A> {
        Merge::new(self, other)
    }

    fn iter(&self) -> Iter<A> {
        Iter::new(self)
    }

    fn hold(&self, initial: A) -> Hold<A> {
        Hold::new(initial, self)
    }
}

impl<A: Send + Sync + Clone, T: HasSource<A>> Event<A> for T {}


pub struct Sink<A> {
    source: Arc<RwLock<Source<A>>>,
}

impl<A: Send> Sink<A> {
    pub fn new() -> Sink<A> {
        Sink { source: Arc::new(RwLock::new(Source::new())) }
    }
}

impl<A: Send + Sync + Clone> Sink<A> {
    pub fn send(&self, a: A) {
        self.source.write().unwrap().send(a);
    }
}

impl<A: Send + Sync + Clone> HasSource<A> for Sink<A> {
    type Source = subject::Source<A>;

    fn source(&self) -> &Arc<RwLock<subject::Source<A>>> {
        &self.source
    }
}


pub struct Map<A, B, F> {
    mapper: Arc<RwLock<Mapper<A, B, F>>>,
}

impl<A, B, F> Map<A, B, F>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          F: Fn(A) -> B + Send + Sync,
{
    pub fn new<E: Event<A>>(event: &E, f: F) -> Map<A, B, F> {
        let map = Map { mapper: Arc::new(RwLock::new(Mapper::new(f))) };
        event.source().write().unwrap().listen(map.mapper.wrap());
        map
    }
}

impl<A, B, F> HasSource<B> for Map<A, B, F>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          F: Fn(A) -> B + Send + Sync,
{
    type Source = Mapper<A, B, F>;

    fn source(&self) -> &Arc<RwLock<Mapper<A, B, F>>> {
        &self.mapper
    }
}


pub struct Filter<A, F> {
    filter: Arc<RwLock<subject::Filter<A, F>>>,
}

impl<A, F> Filter<A, F>
    where A: Send + Sync + Clone,
          F: Fn(&A) -> bool + Send + Sync,
{
    pub fn new<E: Event<A>>(event: &E, f: F) -> Filter<A, F> {
        let filter = Filter {
            filter: Arc::new(RwLock::new(subject::Filter::new(f)))
        };
        event.source().write().unwrap().listen(filter.filter.wrap());
        filter
    }
}

impl<A, F> HasSource<A> for Filter<A, F>
    where A: Send + Sync + Clone,
          F: Fn(&A) -> bool + Send + Sync,
{
    type Source = subject::Filter<A, F>;

    fn source(&self) -> &Arc<RwLock<subject::Filter<A, F>>> {
        &self.filter
    }
}


pub struct Merge<A> {
    source: Arc<RwLock<Source<A>>>,
}

impl<A> Merge<A>
    where A: Send + Sync + Clone,
{
    pub fn new<E1: Event<A>, E2: Event<A>>(event1: &E1, event2: &E2) -> Merge<A> {
        let merge = Merge {
            source: Arc::new(RwLock::new(Source::new()))
        };
        event1.source().write().unwrap().listen(merge.source.wrap());
        event2.source().write().unwrap().listen(merge.source.wrap());
        merge
    }
}

impl<A> HasSource<A> for Merge<A>
    where A: Send + Sync + Clone,
{
    type Source = Source<A>;

    fn source(&self) -> &Arc<RwLock<Source<A>>> {
        &self.source
    }
}


pub struct Snapshot<A, B> {
    source: Arc<RwLock<Snapper<A, B>>>,
}

impl<A, B> Snapshot<A, B>
    where A: Send + Sync + Clone, B: Send + Sync + Clone
{
    pub fn new<Be: Behaviour<A>, Ev: Event<B>>(behaviour: &Be, event: &Ev) -> Snapshot<A, B> {
        let snap = Snapshot {
            source: Arc::new(RwLock::new(Snapper::new(behaviour.sample())))
        };
        behaviour.source().write().unwrap()
            .listen(SnapperWrapper::boxed(&snap.source));
        event.source().write().unwrap().listen(snap.source.wrap());
        snap
    }
}

impl<A, B> HasSource<(A, B)> for Snapshot<A, B>
    where A: Send + Sync, B: Send + Sync
{
    type Source = Snapper<A, B>;

    fn source(&self) -> &Arc<RwLock<Snapper<A, B>>> { &self.source }
}


pub struct Iter<A> {
    recv: Arc<RwLock<Receiver<A>>>,
}

impl<A: Send + Sync + Clone> Iter<A> {
    fn new<E: Event<A>>(event: &E) -> Iter<A> {
        let iter = Iter { recv: Arc::new(RwLock::new(Receiver::new())) };
        event.source().write().unwrap().listen(iter.recv.wrap());
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
    use super::*;

    #[test]
    fn sink() {
        let sink = Sink::new();
        let mut iter = sink.iter();
        sink.send(1);
        sink.send(2);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
    }

    #[test]
    fn map() {
        let sink = Sink::new();
        let triple = sink.map(|x| 3 * x);
        let mut iter = triple.iter();
        sink.send(1);
        assert_eq!(iter.next(), Some(3));
    }

    #[test]
    fn filter() {
        let sink = Sink::new();
        let small = sink.filter(|&x: &i32| x < 11);
        let mut iter = small.iter();
        sink.send(12);
        sink.send(9);
        assert_eq!(iter.next(), Some(9));
    }

    #[test]
    fn merge() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let merge = sink1.merge(&sink2);
        let mut iter = merge.iter();
        sink1.send(12);
        sink2.send(9);
        assert_eq!(iter.next(), Some(12));
        assert_eq!(iter.next(), Some(9));
    }
}
