use std::sync::{Arc, RwLock};
use subject::{Subject, Holder, WrapArc};
use primitives::{Event, HasSource, Snapshot};


pub trait Behaviour<A: Send + Sync + Clone>: HasSource<A> + Sized {
    fn sample(&self) -> A;

    fn snapshot<B, E>(&self, event: &E) -> Snapshot<A, B>
        where B: Send + Sync + Clone, E: Event<B>
    {
        Snapshot::new(self, event)
    }
}


pub struct Hold<A> {
    holder: Arc<RwLock<Holder<A>>>,
}

impl<A: Send + Sync + Clone> Hold<A> {
    pub fn new<E: Event<A>>(initial: A, event: &E) -> Hold<A> {
        let hold = Hold {
            holder: Arc::new(RwLock::new(
                Holder::new(initial, event.source().wrap_as_subject())
            ))
        };
        event.source().write().unwrap().listen(hold.holder.wrap_as_listener());
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

    #[test]
    fn snapshot() {
        let ev1 = Sink::new();
        let beh1 = ev1.hold(5);
        let ev2 = Sink::new();
        let snap = beh1.snapshot(&ev2);
        let mut iter = snap.iter();
        assert_eq!(iter.next(), None);
        ev2.send(4);
        assert_eq!(iter.next(), Some((5, 4)));
        ev1.send(-2);
        assert_eq!(iter.next(), None);
        ev2.send(6);
        assert_eq!(iter.next(), Some((-2, 6)));
    }
}
