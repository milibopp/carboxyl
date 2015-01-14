use std::sync::mpsc::{channel, Sender, Receiver};

pub struct Subject<A> {
    senders: Vec<Sender<A>>,
}

unsafe impl<A: Send> Send for Subject<A> {}
unsafe impl<A: Sync> Sync for Subject<A> {}

impl<A> Subject<A> {
    pub fn new() -> Subject<A> {
        Subject { senders: Vec::new() }
    }
}

impl<A: Send + Clone> Subject<A> {
    pub fn listen(&mut self) -> Receiver<A> {
        let (tx, rx) = channel::<A>();
        self.senders.push(tx);
        rx
    }

    pub fn send(&mut self, a: A) {
        let mut idx_to_remove = vec!();
        for (k, tx) in self.senders.iter().enumerate() {
            match tx.send(a.clone()) {
                Ok(_) => (),
                Err(_) => idx_to_remove.push(k),
            }
        }
        for k in idx_to_remove.into_iter() {
            self.senders.remove(k);
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn two_receivers() {
        let mut sub = Subject::new();
        let r1 = sub.listen();
        let r2 = sub.listen();
        sub.send(3);
        assert_eq!(r1.recv(), Ok(3));
        assert_eq!(r2.recv(), Ok(3));
    }
}
