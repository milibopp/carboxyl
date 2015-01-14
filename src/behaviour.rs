use std::sync::mpsc::Receiver;
use std::sync::RwLock;
use event::Event;

pub struct Behaviour<A> {
    state: RwLock<A>,
    recv: Receiver<A>,
}

impl<A: Send + Sync + Clone> Behaviour<A> {
    pub fn new(initial: A, event: &Event<A>) -> Behaviour<A> {
        Behaviour { state: RwLock::new(initial), recv: event.listen() }
    }

    pub fn state(&self) -> A {
        // Update internal state
        match {
            let mut new_state = None;
            while let Ok(update) = self.recv.try_recv() {
                new_state = Some(update);
            }
            new_state
        } {
            Some(update) => *self.state.write().unwrap() = update,
            None => (),
        }
        self.state.read().unwrap().clone()
    }
}


#[cfg(test)]
mod test {
    use event::Event;
    use super::*;

    #[test]
    fn hold() {
        let sink = Event::new();
        let behaviour = sink.hold(1);
        sink.send(3);
        assert_eq!(behaviour.state(), 3);
    }
}
