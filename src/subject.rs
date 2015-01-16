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

pub struct WeakListenerWrapper<L> {
    weak: Weak<RwLock<L>>
}

impl<L> WeakListenerWrapper<L> {
    pub fn boxed<A>(strong: &Arc<RwLock<L>>) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync,
    {
        Box::new(WeakListenerWrapper { weak: strong.downgrade() })
    }
}

impl<A, L> Listener<A> for WeakListenerWrapper<L>
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


pub struct StrongSubjectWrapper<S> {
    #[allow(dead_code)]
    arc: Arc<RwLock<S>>
}

impl<S> StrongSubjectWrapper<S> {
    pub fn boxed<A>(strong: &Arc<RwLock<S>>) -> Box<Subject<A> + 'static>
        where S: Subject<A>, A: Send + Sync,
    {
        Box::new(StrongSubjectWrapper { arc: strong.clone() })
    }
}

impl<A, S> Subject<A> for StrongSubjectWrapper<S>
    where S: Subject<A> + Send + Sync, A: Send + Sync
{
    fn listen(&mut self, _: Box<Listener<A> + 'static>) {
        panic!("only meant to keep source alive");
    }
}


pub trait WrapArc<L> {
    fn wrap_as_listener<A>(&self) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync;
    fn wrap_as_subject<A>(&self) -> Box<Subject<A> + 'static>
        where L: Subject<A>, A: Send + Sync;
}

impl<L> WrapArc<L> for Arc<RwLock<L>> {
    fn wrap_as_listener<A>(&self) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync
    {
        WeakListenerWrapper::boxed(self)
    }

    fn wrap_as_subject<A>(&self) -> Box<Subject<A> + 'static>
        where L: Subject<A>, A: Send + Sync
    {
        StrongSubjectWrapper::boxed(self)
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
    source: Source<B>,
    #[allow(dead_code)]
    keep_alive: Box<Subject<A> + 'static>,
}

impl<A, B, F: Fn(A) -> B> Mapper<A, B, F> {
    pub fn new(func: F, keep_alive: Box<Subject<A> + 'static>) -> Mapper<A, B, F> {
        Mapper { func: func, source: Source::new(), keep_alive: keep_alive }
    }
}

impl<A, B, F> Subject<B> for Mapper<A, B, F>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          F: Fn(A) -> B + Send + Sync,
{
    fn listen(&mut self, listener: Box<Listener<B> + 'static>) {
        self.source.listen(listener);
    }
}

impl<A, B, F> Listener<A> for Mapper<A, B, F>
    where A: Send + Sync,
          B: Send + Sync + Clone,
          F: Fn(A) -> B + Send + Sync,
{
    fn accept(&mut self, a: A) -> ListenerResult {
        self.source.accept((self.func)(a))
    }
}


pub struct Filter<A, F> {
    func: F,
    source: Source<A>,
    #[allow(dead_code)]
    keep_alive: Box<Subject<A> + 'static>,
}

impl<A, F> Filter<A, F> {
    pub fn new(f: F, keep_alive: Box<Subject<A> + 'static>) -> Filter<A, F> {
        Filter { source: Source::new(), func: f, keep_alive: keep_alive }
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
    #[allow(dead_code)]
    keep_alive: Box<Subject<A> + 'static>,
}

impl<A> Holder<A> {
    pub fn new(initial: A, keep_alive: Box<Subject<A> + 'static>) -> Holder<A> {
        Holder { current: initial, source: Source::new(), keep_alive: keep_alive }
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
    #[allow(dead_code)]
    keep_alive: (Box<Subject<A> + 'static>, Box<Subject<B> + 'static>),
}

impl<A, B> Snapper<A, B> {
    pub fn new(initial: A, keep_alive: (Box<Subject<A> + 'static>, Box<Subject<B> + 'static>)) -> Snapper<A, B> {
        Snapper { current: initial, source: Source::new(), keep_alive: keep_alive }
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


pub struct Merger<A> {
    source: Source<A>,
    #[allow(dead_code)]
    keep_alive: [Box<Subject<A> + 'static>; 2],
}

impl<A> Merger<A> {
    pub fn new(keep_alive: [Box<Subject<A> + 'static>; 2]) -> Merger<A> {
        Merger { source: Source::new(), keep_alive: keep_alive }
    }
}

impl<A: Send + Sync + Clone> Listener<A> for Merger<A> {
    fn accept(&mut self, a: A) -> ListenerResult {
        self.source.accept(a)
    }
}

impl<A: Send + Sync> Subject<A> for Merger<A> {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.source.listen(listener);
    }
}


pub struct WeakSnapperWrapper<A, B> {
    weak: Weak<RwLock<Snapper<A, B>>>,
}

impl<A: Send + Sync, B: Send + Sync> WeakSnapperWrapper<A, B> {
    pub fn boxed(strong: &Arc<RwLock<Snapper<A, B>>>) -> Box<Listener<A> + 'static> {
        Box::new(WeakSnapperWrapper { weak: strong.downgrade() })
    }
}

impl<A: Send + Sync, B: Send + Sync> Listener<A> for WeakSnapperWrapper<A, B> {
    fn accept(&mut self, a: A) -> ListenerResult {
        match self.weak.upgrade() {
            Some(arc) => match arc.write() {
                Ok(mut snapper) => { snapper.current = a; Ok(()) },
                Err(_) => Err(ListenerError::Poisoned),
            },
            None => Err(ListenerError::Disappeared),
        }
    }
}


pub struct Receiver<A> {
    buffer: RingBuf<A>,
    #[allow(dead_code)]
    keep_alive: Box<Subject<A> + 'static>,
}

impl<A> Receiver<A> {
    pub fn new(keep_alive: Box<Subject<A> + 'static>) -> Receiver<A> {
        Receiver { buffer: RingBuf::new(), keep_alive: keep_alive }
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
        let src = Arc::new(RwLock::new(Source::new()));
        let recv = Arc::new(RwLock::new(Receiver::new(src.wrap_as_subject())));
        src.write().unwrap().listen(recv.wrap_as_listener());
        src.write().unwrap().send(3);
        assert_eq!(recv.write().unwrap().next(), Some(3));
    }

    #[test]
    fn map() {
        let src = Arc::new(RwLock::new(Source::new()));
        let map = Arc::new(RwLock::new(Mapper::new(|x: i32| x + 3, src.wrap_as_subject())));
        src.write().unwrap().listen(map.wrap_as_listener());
        let recv = Arc::new(RwLock::new(Receiver::new(map.wrap_as_subject())));
        map.write().unwrap().listen(recv.wrap_as_listener());
        src.write().unwrap().send(3);
        assert_eq!(recv.write().unwrap().next(), Some(6));
    }

    #[test]
    fn fork() {
        let src = Arc::new(RwLock::new(Source::new()));
        let map = Arc::new(RwLock::new(Mapper::new(|x: i32| x + 3, src.wrap_as_subject())));
        src.write().unwrap().listen(map.wrap_as_listener());
        let r1 = Arc::new(RwLock::new(Receiver::new(map.wrap_as_subject())));
        map.write().unwrap().listen(r1.wrap_as_listener());
        let r2 = Arc::new(RwLock::new(Receiver::new(src.wrap_as_subject())));
        src.write().unwrap().listen(r2.wrap_as_listener());
        src.write().unwrap().send(4);
        assert_eq!(r1.write().unwrap().next(), Some(7));
        assert_eq!(r2.write().unwrap().next(), Some(4));
    }

    #[test]
    fn filter() {
        let src = Arc::new(RwLock::new(Source::new()));
        let filter = Arc::new(RwLock::new(Filter::new(|&:x: &i32| *x > 2, src.wrap_as_subject())));
        src.write().unwrap().listen(filter.wrap_as_listener());
        let recv = Arc::new(RwLock::new(Receiver::new(filter.wrap_as_subject())));
        filter.write().unwrap().listen(recv.wrap_as_listener());
        src.write().unwrap().send(1);
        src.write().unwrap().send(3);
        assert_eq!(recv.write().unwrap().next(), Some(3));
    }

    #[test]
    fn holder() {
        let src = Arc::new(RwLock::new(Source::new()));
        let holder = Arc::new(RwLock::new(Holder::new(1, src.wrap_as_subject())));
        src.write().unwrap().listen(holder.wrap_as_listener());
        assert_eq!(holder.write().unwrap().current(), 1);
        src.write().unwrap().send(3);
        assert_eq!(holder.write().unwrap().current(), 3);
    }

    #[test]
    fn snapper() {
        let src1 = Arc::new(RwLock::new(Source::new()));
        let src2 = Arc::new(RwLock::new(Source::new()));
        let snapper = Arc::new(RwLock::new(Snapper::new(3, (src1.wrap_as_subject(), src2.wrap_as_subject()))));
        src1.write().unwrap().listen(WeakSnapperWrapper::boxed(&snapper));
        src2.write().unwrap().listen(snapper.wrap_as_listener());
        let recv = Arc::new(RwLock::new(Receiver::new(snapper.wrap_as_subject())));
        snapper.write().unwrap().listen(recv.wrap_as_listener());
        src2.write().unwrap().send(6.0);
        assert_eq!(recv.write().unwrap().next(), Some((3, 6.0)));
        src1.write().unwrap().send(5);
        assert_eq!(recv.write().unwrap().next(), None);
        src2.write().unwrap().send(-4.0);
        assert_eq!(recv.write().unwrap().next(), Some((5, -4.0)));
    }
}
