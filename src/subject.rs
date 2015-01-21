//! Low-level push primitives

#![allow(missing_docs)]

use std::sync::{Arc, RwLock, Weak, Mutex};
use std::sync::mpsc::Sender;
use Cell;
use transaction::register_callback;


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
        self.arc.write().ok().expect("StrongSubjectWrapper::listen").listen(listener);
    }
}

impl<A, S> Sample<A> for StrongSubjectWrapper<S>
    where S: Sample<A> + Send + Sync, A: Send + Sync
{
    fn sample(&self) -> A {
        self.arc.write().ok().expect("StrongSubjectWrapper::sample").sample()
    }
}


pub trait WrapArc<L> {
    fn wrap_as_listener<A>(&self) -> Box<Listener<A> + 'static>
        where L: Listener<A>, A: Send + Sync;
    fn wrap_as_subject<A>(&self) -> KeepAlive<A>
        where L: Subject<A>, A: Send + Sync + Clone;
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
        use std::mem;
        let mut new_listeners = vec!();
        mem::swap(&mut new_listeners, &mut self.listeners);
        self.listeners = new_listeners
            .into_iter()
            .filter_map(|mut listener| {
                let result = listener.accept(a.clone());
                match result {
                    Ok(_) => Some(listener),
                    Err(_) => None,
                }
            })
            .collect();
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


pub struct Filter<A> {
    source: Source<A>,
    #[allow(dead_code)]
    keep_alive: KeepAlive<Option<A>>,
}

impl<A> Filter<A> {
    pub fn new(keep_alive: KeepAlive<Option<A>>) -> Filter<A> {
        Filter { source: Source::new(), keep_alive: keep_alive }
    }
}

impl<A: Send + Sync> Subject<A> for Filter<A> {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.source.listen(listener);
    }
}

impl<A> Listener<Option<A>> for Filter<A>
    where A: Send + Sync + Clone,
{
    fn accept(&mut self, a: Option<A>) -> ListenerResult {
        match a {
            Some(a) => self.source.accept(a),
            None => Ok(()),
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
    update: Option<A>,
    source: Source<(A, B)>,
    #[allow(dead_code)]
    keep_alive: (KeepAliveSample<A>, KeepAlive<B>),
}

impl<A, B> Snapper<A, B> {
    pub fn new(initial: A, keep_alive: (KeepAliveSample<A>, KeepAlive<B>)) -> Snapper<A, B> {
        Snapper {
            current: initial,
            update: None,
            source: Source::new(),
            keep_alive: keep_alive
        }
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
                Ok(mut snapper) => {
                    snapper.update = Some(a);
                    let weak = self.weak.clone();
                    register_callback(move || {
                        match weak.upgrade() {
                            Some(arc) => {
                                let mut snapper = arc.write().ok()
                                    .expect("snapshot too poisonous for callback");
                                match snapper.update.take() {
                                    Some(up) => snapper.current = up,
                                    None => (),
                                }
                            }
                            None => (),
                        }
                    });
                    Ok(())
                },
                Err(_) => Err(ListenerError::Poisoned),
            },
            None => Err(ListenerError::Disappeared),
        }
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


pub struct CellSwitcher<A> {
    current: Cell<A>,
    source: Source<A>,
    #[allow(dead_code)]
    keep_alive: KeepAliveSample<Cell<A>>,
}

impl<A> CellSwitcher<A> {
    pub fn new(initial: Cell<A>, keep_alive: KeepAliveSample<Cell<A>>) -> CellSwitcher<A> {
        CellSwitcher { current: initial, source: Source::new(), keep_alive: keep_alive }
    }
}

impl<A: Send + Sync + Clone> Listener<Cell<A>> for CellSwitcher<A> {
    fn accept(&mut self, cell: Cell<A>) -> ListenerResult {
        self.current = cell;
        self.source.accept(self.current.sample_nocommit())
    }
}

impl<A: Send + Sync + Clone> Subject<A> for CellSwitcher<A> {
    fn listen(&mut self, listener: Box<Listener<A> + 'static>) {
        self.source.listen(listener);
    }
}

impl<A: Send + Sync + Clone> Sample<A> for CellSwitcher<A> {
    fn sample(&self) -> A {
        self.current.sample_nocommit()
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


pub struct ChannelBuffer<A> {
    sender: Mutex<Sender<A>>,
    #[allow(dead_code)]
    keep_alive: KeepAlive<A>,
}

impl<A: Send> ChannelBuffer<A> {
    pub fn new(sender: Sender<A>, keep_alive: KeepAlive<A>) -> ChannelBuffer<A> {
        ChannelBuffer { sender: Mutex::new(sender), keep_alive: keep_alive }
    }
}

impl<A: Send + Sync> Listener<A> for ChannelBuffer<A> {
    fn accept(&mut self, a: A) -> ListenerResult {
        match self.sender.lock() {
            Ok(sender) => match sender.send(a) {
                Ok(_) => Ok(()),
                Err(_) => Err(ListenerError::Disappeared),
            },
            Err(_) => Err(ListenerError::Poisoned),
        }
    }
}


#[cfg(test)]
mod test {
    use std::sync::{Arc, RwLock, mpsc};
    use transaction::commit;
    use super::*;

    #[test]
    fn src_recv() {
        let src = Arc::new(RwLock::new(Source::new()));
        let (tx, rx) = mpsc::channel();
        let recv = Arc::new(RwLock::new(ChannelBuffer::new(tx, src.wrap_as_subject())));
        src.write().unwrap().listen(recv.wrap_as_listener());
        src.write().unwrap().send(3);
        assert_eq!(rx.recv(), Ok(3));
    }

    #[test]
    fn map() {
        let src = Arc::new(RwLock::new(Source::new()));
        let map = Arc::new(RwLock::new(Mapper::new(|x: i32| x + 3, src.wrap_as_subject())));
        src.write().unwrap().listen(map.wrap_as_listener());
        let (tx, rx) = mpsc::channel();
        let recv = Arc::new(RwLock::new(ChannelBuffer::new(tx, map.wrap_as_subject())));
        map.write().unwrap().listen(recv.wrap_as_listener());
        src.write().unwrap().send(3);
        assert_eq!(rx.recv(), Ok(6));
    }

    #[test]
    fn fork() {
        let src = Arc::new(RwLock::new(Source::new()));
        let map = Arc::new(RwLock::new(Mapper::new(|x: i32| x + 3, src.wrap_as_subject())));
        src.write().unwrap().listen(map.wrap_as_listener());
        let (tx1, rx1) = mpsc::channel();
        let recv1 = Arc::new(RwLock::new(ChannelBuffer::new(tx1, map.wrap_as_subject())));
        map.write().unwrap().listen(recv1.wrap_as_listener());
        let (tx2, rx2) = mpsc::channel();
        let recv2 = Arc::new(RwLock::new(ChannelBuffer::new(tx2, src.wrap_as_subject())));
        src.write().unwrap().listen(recv2.wrap_as_listener());
        src.write().unwrap().send(4);
        assert_eq!(rx1.recv(), Ok(7));
        assert_eq!(rx2.recv(), Ok(4));
    }

    #[test]
    fn filter() {
        let src = Arc::new(RwLock::new(Source::new()));
        let filter = Arc::new(RwLock::new(Filter::new(src.wrap_as_subject())));
        src.write().unwrap().listen(filter.wrap_as_listener());
        let (tx, rx) = mpsc::channel();
        let recv = Arc::new(RwLock::new(ChannelBuffer::new(tx, filter.wrap_as_subject())));
        filter.write().unwrap().listen(recv.wrap_as_listener());
        src.write().unwrap().send(None);
        src.write().unwrap().send(Some(3));
        assert_eq!(rx.recv(), Ok(3));
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
        let src1 = Arc::new(RwLock::new(Source::<i32>::new()));
        let src2 = Arc::new(RwLock::new(Source::<f64>::new()));
        let holder = Arc::new(RwLock::new(Holder::new(1, src1.wrap_as_subject())));
        src1.write().unwrap().listen(holder.wrap_as_listener());
        let snapper = Arc::new(RwLock::new(Snapper::new(3, (holder.wrap_as_sampling_subject(), src2.wrap_as_subject()))));
        src1.write().unwrap().listen(WeakSnapperWrapper::boxed(&snapper));
        src2.write().unwrap().listen(snapper.wrap_as_listener());
        let (tx, rx) = mpsc::channel();
        let recv = Arc::new(RwLock::new(ChannelBuffer::new(tx, snapper.wrap_as_subject())));
        snapper.write().unwrap().listen(recv.wrap_as_listener());
        commit((), |_| src2.write().unwrap().send(6.0));
        assert_eq!(rx.recv(), Ok((3, 6.0)));
        commit((), |_| src1.write().unwrap().send(5));
        commit((), |_| src2.write().unwrap().send(-4.0));
        assert_eq!(rx.recv(), Ok((5, -4.0)));
    }
}
