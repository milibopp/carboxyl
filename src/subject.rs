//! Low-level push primitives

use std::sync::{Arc, RwLock, Weak};
use std::collections::RingBuf;


pub enum ListenerError {
    Disappeared,
    Poisoned,
}

pub trait Listener<A>: Send + Sync {
    fn accept(&mut self, a: A) -> Result<(), ListenerError>;
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
    fn accept(&mut self, a: A) -> Result<(), ListenerError> {
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
    fn accept(&mut self, a: A) -> Result<(), ListenerError> {
        self.subject.send((self.func)(a));
        Ok(())
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
    fn accept(&mut self, a: A) -> Result<(), ListenerError> {
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
}
