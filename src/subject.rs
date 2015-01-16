//! Low-level push primitives

use std::sync::{Arc, RwLock, Weak};
use std::collections::RingBuf;


#[derive(Show)]
pub enum ListenerError {
    Disappeared,
    Poisoned,
}

pub type ListenerResult = Result<(), ListenerError>;

pub trait Listener<A>: Send + Sync {
    fn accept(&mut self, a: A) -> ListenerResult;
}

pub struct ListenerWrapper<L> {
    weak: Weak<RwLock<L>>
}

impl<L> ListenerWrapper<L> {
    pub fn boxed<A>(strong: &Arc<RwLock<L>>) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync,
    {
        Box::new(ListenerWrapper { weak: strong.downgrade() })
    }
}

impl<A, L> Listener<A> for ListenerWrapper<L>
    where L: Listener<A> + Send + Sync, A: Send + Sync
{
    fn accept(&mut self, a: A) -> ListenerResult {
        match self.weak.upgrade() {
            Some(listener) => match listener.write() {
                Ok(mut listener) => listener.accept(a),
                Err(_) => Err(ListenerError::Poisoned),
            },
            None => Err(ListenerError::Disappeared),
        }
    }
}


pub trait WrapListener<L> {
    fn wrap<A>(&self) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync;
}

impl<L> WrapListener<L> for Arc<RwLock<L>> {
    fn wrap<A>(&self) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync
    {
        ListenerWrapper::boxed(self)
    }
}


pub trait Subject<A>: Send + Sync {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>);
}


pub struct Source<A> {
    listeners: Vec<Box<Listener<A> + 'static>>,
}

impl<A> Source<A> {
    pub fn new() -> Source<A> {
        Source { listeners: Vec::new() }
    }
}

impl<A: Send + Sync + Clone> Source<A> {
    pub fn send(&mut self, a: A) {
        let mut idx_to_remove = vec!();
        for (k, listener) in self.listeners.iter_mut().enumerate() {
            if listener.accept(a.clone()).is_err() {
                idx_to_remove.push(k);
            }
        }
        for k in idx_to_remove.into_iter() {
            self.listeners.remove(k);
        }
    }
}

impl<A: Send + Sync + Clone> Listener<A> for Source<A> {
    fn accept(&mut self, a: A) -> ListenerResult {
        self.send(a);
        Ok(())
    }
}

impl<A: Send + Sync> Subject<A> for Source<A> {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.listeners.push(listener);
    }
}


pub struct Mapper<A, B, F: Fn(A) -> B> {
    func: F,
    subject: Source<B>,
}

impl<A, B, F: Fn(A) -> B> Mapper<A, B, F> {
    pub fn new(func: F) -> Mapper<A, B, F> {
        Mapper { func: func, subject: Source::new() }
    }
}

impl<A, B, F> Subject<B> for Mapper<A, B, F>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          F: Fn(A) -> B + Send + Sync,
{
    fn listen(&mut self, listener: Box<Listener<B> + 'static>) {
        self.subject.listen(listener);
    }
}

impl<A, B, F> Listener<A> for Mapper<A, B, F>
    where B: Send + Sync + Clone,
          F: Fn(A) -> B + Send + Sync,
{
    fn accept(&mut self, a: A) -> ListenerResult {
        self.subject.accept((self.func)(a))
    }
}


pub struct Filter<A, F> {
    func: F,
    source: Source<A>,
}

impl<A, F> Filter<A, F> {
    pub fn new(f: F) -> Filter<A, F> {
        Filter { source: Source::new(), func: f }
    }
}

impl<A: Send + Sync, F: Send + Sync> Subject<A> for Filter<A, F> {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.source.listen(listener);
    }
}

impl<A, F> Listener<A> for Filter<A, F>
    where A: Send + Sync + Clone,
          F: Fn(&A) -> bool + Send + Sync,
{
    fn accept(&mut self, a: A) -> ListenerResult {
        if (self.func)(&a) {
            self.source.accept(a)
        } else {
            Ok(())
        }
    }
}


pub struct Holder<A> {
    current: A,
    source: Source<A>,
}

impl<A> Holder<A> {
    pub fn new(initial: A) -> Holder<A> {
        Holder { current: initial, source: Source::new() }
    }
}

impl<A: Clone> Holder<A> {
    pub fn current(&self) -> A { self.current.clone() }
}

impl<A: Send + Sync> Subject<A> for Holder<A> {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.source.listen(listener);
    }
}

impl<A: Send + Sync + Clone> Listener<A> for Holder<A> {
    fn accept(&mut self, a: A) -> ListenerResult {
        self.current = a.clone();
        self.source.accept(a)
    }
}


pub struct Snapper<A, B> {
    current: A,
    source: Source<(A, B)>,
}

impl<A, B> Snapper<A, B> {
    pub fn new(initial: A) -> Snapper<A, B> {
        Snapper { current: initial, source: Source::new() }
    }
}

pub struct SnapperWrapper<A, B> {
    weak: Weak<RwLock<Snapper<A, B>>>,
}

impl<A: Send + Sync, B: Send + Sync> SnapperWrapper<A, B> {
    pub fn boxed(strong: &Arc<RwLock<Snapper<A, B>>>) -> Box<Listener<A> + 'static> {
        Box::new(SnapperWrapper { weak: strong.downgrade() })
    }
}

impl<A: Send + Sync, B: Send + Sync> Listener<A> for SnapperWrapper<A, B> {
    fn accept(&mut self, a: A) -> ListenerResult {
        let x = match self.weak.upgrade() {
            Some(arc) => match arc.write() {
                Ok(mut snapper) => { snapper.current = a; Ok(()) },
                Err(_) => Err(ListenerError::Poisoned),
            },
            None => Err(ListenerError::Disappeared),
        };
        println!("{:?}", x);
        x
    }
}

impl<A: Send + Sync + Clone, B: Send + Sync + Clone> Listener<B> for Snapper<A, B> {
    fn accept(&mut self, b: B) -> ListenerResult {
        self.source.accept((self.current.clone(), b))
    }
}

impl<A: Send + Sync, B: Send + Sync> Subject<(A, B)> for Snapper<A, B> {
    fn listen(&mut self, listener: Box<Listener<(A, B)> + 'static>) {
        self.source.listen(listener);
    }
}


pub struct Receiver<A> {
    buffer: RingBuf<A>,
}

impl<A> Receiver<A> {
    pub fn new() -> Receiver<A> {
        Receiver { buffer: RingBuf::new() }
    }
}

impl<A> Iterator for Receiver<A> {
    type Item = A;
    fn next(&mut self) -> Option<A> {
        self.buffer.pop_front()
    }
}

impl<A: Send + Sync> Subject<()> for Receiver<A> {
    fn listen(&mut self, _: Box<Listener<()> + 'static>) {}
}

impl<A: Send + Sync> Listener<A> for Receiver<A> {
    fn accept(&mut self, a: A) -> ListenerResult {
        self.buffer.push_back(a);
        Ok(())
    }
}


#[cfg(test)]
mod test {
    use std::sync::{Arc, RwLock};
    use super::*;

    #[test]
    fn src_recv() {
        let mut src = Source::new();
        let recv = Arc::new(RwLock::new(Receiver::new()));
        src.listen(recv.wrap());
        src.send(3);
        assert_eq!(recv.write().unwrap().next(), Some(3));
    }

    #[test]
    fn map() {
        let mut src = Source::new();
        let map = Arc::new(RwLock::new(Mapper::new(|x: i32| x + 3)));
        src.listen(map.wrap());
        let recv = Arc::new(RwLock::new(Receiver::new()));
        map.write().unwrap().listen(recv.wrap());
        src.send(3);
        assert_eq!(recv.write().unwrap().next(), Some(6));
    }

    #[test]
    fn fork() {
        let mut src = Source::new();
        let map = Arc::new(RwLock::new(Mapper::new(|x: i32| x + 3)));
        src.listen(map.wrap());
        let r1 = Arc::new(RwLock::new(Receiver::new()));
        map.write().unwrap().listen(r1.wrap());
        let r2 = Arc::new(RwLock::new(Receiver::new()));
        src.listen(r2.wrap());
        src.send(4);
        assert_eq!(r1.write().unwrap().next(), Some(7));
        assert_eq!(r2.write().unwrap().next(), Some(4));
    }

    #[test]
    fn filter() {
        let mut src = Source::new();
        let filter = Arc::new(RwLock::new(Filter::new(|&:x: &i32| *x > 2)));
        src.listen(filter.wrap());
        let recv = Arc::new(RwLock::new(Receiver::new()));
        filter.write().unwrap().listen(recv.wrap());
        src.send(1);
        src.send(3);
        assert_eq!(recv.write().unwrap().next(), Some(3));
    }

    #[test]
    fn holder() {
        let mut src = Source::new();
        let holder = Arc::new(RwLock::new(Holder::new(1)));
        src.listen(holder.wrap());
        assert_eq!(holder.write().unwrap().current(), 1);
        src.send(3);
        assert_eq!(holder.write().unwrap().current(), 3);
    }

    #[test]
    fn snapper() {
        let mut src1 = Source::new();
        let mut src2 = Source::new();
        let snapper = Arc::new(RwLock::new(Snapper::new(3)));
        src1.listen(SnapperWrapper::boxed(&snapper));
        src2.listen(snapper.wrap());
        let recv = Arc::new(RwLock::new(Receiver::new()));
        snapper.write().unwrap().listen(recv.wrap());
        src2.send(6);
        assert_eq!(recv.write().unwrap().next(), Some((3, 6)));
        src1.send(5);
        assert_eq!(recv.write().unwrap().next(), None);
        src2.send(-4);
        assert_eq!(recv.write().unwrap().next(), Some((5, -4)));
    }
}
