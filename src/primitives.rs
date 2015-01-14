use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::thread::Thread;
use subject::Subject;


pub trait Event<A> {
    fn listen(&self) -> Receiver<A>;
    fn map<B: Send + Sync + Clone, F: Fn(A) -> B + Send>(&self, f: F) -> Map<B>;
}

pub struct Sink<A> {
    subject: Arc<RwLock<Subject<A>>>,
}

impl<A: Send + Sync + Clone> Sink<A> {
    pub fn new() -> Sink<A> {
        Sink { subject: Arc::new(RwLock::new(Subject::new())) }
    }

    pub fn send(&self, a: A) {
        self.subject.write().unwrap().send(a);
    }
}

impl<A: Send + Sync + Clone> Event<A> for Sink<A> {
    fn listen(&self) -> Receiver<A> {
        self.subject.write().unwrap().listen()
    }

    fn map<B: Send + Sync + Clone, F: Fn(A) -> B + Send>(&self, f: F) -> Map<B> {
        Map::new(self.subject.write().unwrap().listen(), f)
    }
}

pub struct Map<A> {
    subject: Arc<RwLock<Subject<A>>>,
}

impl<B: Send + Sync + Clone> Map<B> {
    pub fn new<A, F>(source: Receiver<A>, f: F) -> Map<B>
        where A: Send + Sync,
              F: Fn(A) -> B + Send,
    {
        let subject = Arc::new(RwLock::new(Subject::new()));
        {
            let subject = subject.clone();
            Thread::spawn(move || {
                for a in source.iter() {
                    subject.write().unwrap().send(f(a))
                }
            });
        }
        Map { subject: subject }
    }
}

impl<B: Send + Sync + Clone> Event<B> for Map<B> {
    fn listen(&self) -> Receiver<B> {
        self.subject.write().unwrap().listen()
    }

    fn map<C: Send + Sync + Clone, G: Fn(B) -> C + Send>(&self, g: G) -> Map<C> {
        Map::new(self.subject.write().unwrap().listen(), g)
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sink() {
        let sink = Sink::new();
        let r = sink.listen();
        sink.send(1);
        assert_eq!(r.recv(), Ok(1))
    }

    #[test]
    fn map() {
        let sink = Sink::new();
        let triple = sink.map(|x| 3 * x);
        let r = triple.listen();
        sink.send(1);
        assert_eq!(r.recv(), Ok(3));
    }
}
