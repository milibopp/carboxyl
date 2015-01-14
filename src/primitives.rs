use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::thread::Thread;
use subject::Subject;


pub struct Event<A> {
    subject: Arc<RwLock<Subject<A>>>,
}

impl<A: Send + Sync + Clone> Event<A> {
    pub fn new() -> Event<A> {
        Event { subject: Arc::new(RwLock::new(Subject::new())) }
    }

    pub fn send(&self, a: A) {
        self.subject.write().unwrap().send(a);
    }

    pub fn listen(&self) -> Receiver<A> {
        self.subject.write().unwrap().listen()
    }

    pub fn map<B: Send + Sync + Clone, F: Fn(A) -> B + Send>(&self, f: F) -> Event<B> {
        let event = Event::new();
        let source = self.listen();
        let subject = event.subject.clone();
        Thread::spawn(move || {
            for a in source.iter() {
                subject.write().unwrap().send(f(a))
            }
        });
        event
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sink() {
        let sink = Event::new();
        let r = sink.listen();
        sink.send(1);
        assert_eq!(r.recv(), Ok(1))
    }

    #[test]
    fn map() {
        let sink = Event::new();
        let triple = sink.map(|x| 3 * x);
        let r = triple.listen();
        sink.send(1);
        assert_eq!(r.recv(), Ok(3));
    }
}
