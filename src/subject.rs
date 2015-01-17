//! Low-level push primitives

#![allow(missing_docs)]

use std::sync::{Arc, RwLock, Weak};
use std::collections::RingBuf;
use primitives::Cell;


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
        let x = match self.weak.upgrade() {
            Some(listener) => match listener.write() {
                Ok(mut listener) => listener.accept(a),
                Err(_) => Err(ListenerError::Poisoned),
            },
            None => Err(ListenerError::Disappeared),
        };
        println!("{:?}", x);
        x
    }
}


type KeepAlive<A> = Arc<RwLock<Box<Subject<A> + 'static>>>;
type KeepAliveSample<A> = Arc<RwLock<Box<SamplingSubject<A> + 'static>>>;


pub struct StrongSubjectWrapper<S> {
    #[allow(dead_code)]
    arc: Arc<RwLock<S>>
}

impl<A, S> Subject<A> for StrongSubjectWrapper<S>
    where S: Subject<A> + Send + Sync, A: Send + Sync
{
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.arc.write().unwrap().listen(listener);
    }
}

impl<A, S> Sample<A> for StrongSubjectWrapper<S>
    where S: Sample<A> + Send + Sync, A: Send + Sync
{
    fn sample(&self) -> A {
        self.arc.write().unwrap().sample()
    }
}


pub trait WrapArc<L> {
    fn wrap_as_listener<A>(&self) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync;
    fn wrap_as_subject<A>(&self) -> KeepAlive<A>
        where L: Subject<A>, A: Send + Sync;
    fn wrap_into_subject<A>(self) -> KeepAlive<A>
        where L: Subject<A>, A: Send + Sync + Clone;
    fn wrap_as_sampling_subject<A>(&self) -> KeepAliveSample<A>
        where L: SamplingSubject<A>, A: Send + Sync + Clone;
    fn wrap_into_sampling_subject<A>(self) -> KeepAliveSample<A>
        where L: SamplingSubject<A>, A: Send + Sync + Clone;
}

impl<L> WrapArc<L> for Arc<RwLock<L>> {
    fn wrap_as_listener<A>(&self) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync
    {
        WeakListenerWrapper::boxed(self)
    }

    fn wrap_as_subject<A>(&self) -> KeepAlive<A>
        where L: Subject<A>, A: Send + Sync + Clone
    {
        Arc::new(RwLock::new(Box::new(StrongSubjectWrapper { arc: self.clone() })))
    }

    fn wrap_into_subject<A>(self) -> KeepAlive<A>
        where L: Subject<A>, A: Send + Sync + Clone
    {
        Arc::new(RwLock::new(Box::new(StrongSubjectWrapper { arc: self })))
    }

    fn wrap_as_sampling_subject<A>(&self) -> KeepAliveSample<A>
        where L: SamplingSubject<A>, A: Send + Sync + Clone
    {
        Arc::new(RwLock::new(Box::new(StrongSubjectWrapper { arc: self.clone() })))
    }

    fn wrap_into_sampling_subject<A>(self) -> KeepAliveSample<A>
        where L: SamplingSubject<A>, A: Send + Sync + Clone
    {
        Arc::new(RwLock::new(Box::new(StrongSubjectWrapper { arc: self })))
    }
}


pub trait Subject<A>: Send + Sync {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>);
}


pub trait Sample<A> {
    fn sample(&self) -> A;
}


pub trait SamplingSubject<A>: Sample<A> + Subject<A> {}

impl<A, T: Sample<A> + Subject<A>> SamplingSubject<A> for T {}


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
    keep_alive: KeepAlive<A>,
}

impl<A, B, F: Fn(A) -> B> Mapper<A, B, F> {
    pub fn new(func: F, keep_alive: KeepAlive<A>) -> Mapper<A, B, F> {
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
    keep_alive: KeepAlive<A>,
}

impl<A, F> Filter<A, F> {
    pub fn new(f: F, keep_alive: KeepAlive<A>) -> Filter<A, F> {
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
    keep_alive: KeepAlive<A>,
}

impl<A> Holder<A> {
    pub fn new(initial: A, keep_alive: KeepAlive<A>) -> Holder<A> {
        Holder { current: initial, source: Source::new(), keep_alive: keep_alive }
    }
}

impl<A: Clone> Sample<A> for Holder<A> {
    fn sample(&self) -> A { self.current.clone() }
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
    keep_alive: (KeepAliveSample<A>, KeepAlive<B>),
}

impl<A, B> Snapper<A, B> {
    pub fn new(initial: A, keep_alive: (KeepAliveSample<A>, KeepAlive<B>)) -> Snapper<A, B> {
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
    keep_alive: [KeepAlive<A>; 2],
}

impl<A> Merger<A> {
    pub fn new(keep_alive: [KeepAlive<A>; 2]) -> Merger<A> {
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


pub struct CellSwitcher<A> {
    current: A,
    source: Source<A>,
    #[allow(dead_code)]
    keep_alive: KeepAliveSample<Cell<A>>,
}

impl<A> CellSwitcher<A> {
    pub fn new(initial: A, keep_alive: KeepAliveSample<Cell<A>>) -> CellSwitcher<A> {
        CellSwitcher { current: initial, source: Source::new(), keep_alive: keep_alive }
    }
}

impl<A: Send + Sync + Clone> Listener<Cell<A>> for CellSwitcher<A> {
    fn accept(&mut self, b: Cell<A>) -> ListenerResult {
        self.current = b.sample();
        self.source.accept(self.current.clone())
    }
}

impl<A: Send + Sync + Clone> Subject<A> for CellSwitcher<A> {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.source.listen(listener);
    }
}

impl<A: Clone> Sample<A> for CellSwitcher<A> {
    fn sample(&self) -> A {
        self.current.clone()
    }
}


pub struct Lift2<A, B, C, F> {
    current: (A, B),
    f: F,
    source: Source<C>,
    #[allow(dead_code)]
    keep_alive: (KeepAliveSample<A>, KeepAliveSample<B>),
}

impl<A, B, C: Send + Sync, F> Lift2<A, B, C, F> {
    pub fn new(initial: (A, B), f: F, keep_alive: (KeepAliveSample<A>, KeepAliveSample<B>))
        -> Lift2<A, B, C, F>
    {
        Lift2 {
            current: initial,
            f: f,
            source: Source::new(),
            keep_alive: keep_alive
        }
    }
}

impl<A, B, C, F> Subject<C> for Lift2<A, B, C, F>
    where A: Send + Sync,
          B: Send + Sync,
          C: Send + Sync + Clone,
          F: Send + Sync,
{
    fn listen(&mut self, listener: Box<Listener<C> + 'static>) {
        self.source.listen(listener);
    }
}

impl<A, B, C, F> Listener<A> for Lift2<A, B, C, F>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          C: Send + Sync + Clone,
          F: Fn(A, B) -> C + Send + Sync,
{
    fn accept(&mut self, a: A) -> ListenerResult {
        self.current.0 = a;
        let (a, b) = self.current.clone();
        self.source.accept((self.f)(a, b))
    }
}

impl<A, B, C, F> Sample<C> for Lift2<A, B, C, F>
    where A: Clone,
          B: Clone,
          F: Fn(A, B) -> C
{
    fn sample(&self) -> C {
        let (a, b) = self.current.clone();
        (self.f)(a, b)
    }
}


pub struct WeakLift2Wrapper<A, B, C, F> {
    weak: Weak<RwLock<Lift2<A, B, C, F>>>,
}

impl<A, B, C, F> WeakLift2Wrapper<A, B, C, F>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          C: Send + Sync + Clone,
          F: Fn(A, B) -> C + Send + Sync,
{
    pub fn boxed(strong: &Arc<RwLock<Lift2<A, B, C, F>>>) -> Box<Listener<B> + 'static> {
        Box::new(WeakLift2Wrapper { weak: strong.downgrade() })
    }
}

impl<A, B, C, F> Listener<B> for WeakLift2Wrapper<A, B, C, F>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          C: Send + Sync + Clone,
          F: Fn(A, B) -> C + Send + Sync,
{
    fn accept(&mut self, b: B) -> ListenerResult {
        match self.weak.upgrade() {
            Some(arc) => match arc.write() {
                Ok(mut snapper) => {
                    snapper.current.1 = b;
                    let (a, b) = snapper.current.clone();
                    let c = (snapper.f)(a, b);
                    snapper.source.accept(c)
                },
                Err(_) => Err(ListenerError::Poisoned),
            },
            None => Err(ListenerError::Disappeared),
        }
    }
}


pub struct Receiver<A> {
    buffer: RingBuf<A>,
    #[allow(dead_code)]
    keep_alive: KeepAlive<A>,
}

impl<A> Receiver<A> {
    pub fn new(keep_alive: KeepAlive<A>) -> Receiver<A> {
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
        assert_eq!(holder.write().unwrap().sample(), 1);
        src.write().unwrap().send(3);
        assert_eq!(holder.write().unwrap().sample(), 3);
    }

    #[test]
    fn snapper() {
        let src1 = Arc::new(RwLock::new(Source::new()));
        let src2 = Arc::new(RwLock::new(Source::new()));
        let holder = Arc::new(RwLock::new(Holder::new(1, src1.wrap_as_subject())));
        src1.write().unwrap().listen(holder.wrap_as_listener());
        let snapper = Arc::new(RwLock::new(Snapper::new(3, (holder.wrap_as_sampling_subject(), src2.wrap_as_subject()))));
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
