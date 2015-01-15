//! Low-level push primitives

use std::sync::{RwLock, Weak};
use std::collections::RingBuf;


pub trait Subject<L> {
    fn listen(&mut self, listener: Weak<RwLock<L>>);
}


pub trait Listener<A>: Send + Sync {
    fn accept(&mut self, a: A);
}

impl<A, L: Listener<A> + ?Sized> Listener<A> for Box<L> {
    fn accept(&mut self, a: A) { self.accept(a); }
}


pub struct Source<L> {
    listeners: Vec<Weak<RwLock<L>>>,
}

unsafe impl<L: Send> Send for Source<L> {}
unsafe impl<L: Sync> Sync for Source<L> {}

impl<L> Source<L> {
    pub fn new() -> Source<L> {
        Source { listeners: Vec::new() }
    }
}

impl<L> Subject<L> for Source<L> {
    fn listen(&mut self, listener: Weak<RwLock<L>>) {
        self.listeners.push(listener);
    }
}

impl<L: Send + Sync> Source<L> {
    pub fn send<A: Clone>(&mut self, a: A)
        where L: Listener<A>
    {
        let mut idx_to_remove = vec!();
        for (k, listener) in self.listeners.iter().enumerate() {
            match listener.upgrade() {
                Some(l) => l.write().unwrap().accept(a.clone()),
                None => idx_to_remove.push(k),
            }
        }
        for k in idx_to_remove.into_iter() {
            self.listeners.remove(k);
        }
    }
}


pub struct Mapper<A, B, F: Fn(A) -> B, L> {
    func: F,
    subject: Source<L>,
}

impl<A, B, F: Fn(A) -> B, L> Mapper<A, B, F, L> {
    pub fn new(func: F) -> Mapper<A, B, F, L> {
        Mapper { func: func, subject: Source::new() }
    }
}

impl<A, B, F, L> Subject<L> for Mapper<A, B, F, L> {
    fn listen(&mut self, listener: Weak<RwLock<L>>) {
        self.subject.listen(listener);
    }
}

impl<A, B, F, L> Listener<A> for Mapper<A, B, F, L>
    where L: Listener<B> + Send + Sync,
          F: Fn(A) -> B + Send + Sync,
          B: Clone,
{
    fn accept(&mut self, a: A) {
        self.subject.send((self.func)(a));
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

impl<A: Send + Sync> Listener<A> for Receiver<A> {
    fn accept(&mut self, a: A) {
        self.buffer.push_back(a);
    }
}

impl<A> Iterator for Receiver<A> {
    type Item = A;
    fn next(&mut self) -> Option<A> {
        self.buffer.pop_front()
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
        src.listen(recv.downgrade());
        src.send(3);
        assert_eq!(recv.write().unwrap().next(), Some(3));
    }

    #[test]
    fn map() {
        let mut src = Source::new();
        let map = Arc::new(RwLock::new(Mapper::new(|x: i32| x + 3)));
        src.listen(map.downgrade());
        let recv = Arc::new(RwLock::new(Receiver::new()));
        map.write().unwrap().listen(recv.downgrade());
        src.send(3);
        assert_eq!(recv.write().unwrap().next(), Some(6));
    }

}
