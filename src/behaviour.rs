use std::sync::{Arc, RwLock};
use subject::{Subject, Holder, WrapListener};
use primitives::{Event, HasSource};


pub trait Behaviour<A: Send + Sync + Clone>: HasSource<A> {
    fn sample(&self) -> A;
}


pub struct Hold<A> {
    holder: Arc<RwLock<Holder<A>>>,
}

impl<A: Send + Sync + Clone> Hold<A> {
    pub fn new<E: Event<A>>(initial: A, event: &E) -> Hold<A> {
        let hold = Hold { holder: Arc::new(RwLock::new(Holder::new(initial))) };
        event.source().write().unwrap().listen(hold.holder.wrap());
        hold
    }
}

impl<A: Send + Sync> HasSource<A> for Hold<A> {
    type Source = Holder<A>;

    fn source(&self) -> &Arc<RwLock<Holder<A>>> {
        &self.holder
    }
}

impl<A: Send + Sync + Clone> Behaviour<A> for Hold<A> {
    fn sample(&self) -> A {
        self.holder.write().unwrap().current()
    }
}


#[cfg(test)]
mod test {
    use primitives::{Event, Sink};
    use super::*;

    #[test]
    fn hold() {
        let ea = Sink::new();
        let ba = ea.hold(3);
        assert_eq!(ba.sample(), 3);
        ea.send(4);
        assert_eq!(ba.sample(), 4);
    }
}
