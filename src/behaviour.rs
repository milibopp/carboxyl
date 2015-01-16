use std::sync::{Arc, RwLock};
use subject::{Subject, Holder, WrapArc, Sample, BehaviourSwitcher};
use primitives::{Event, HasSource, Snapshot};


pub trait Behaviour<A: Send + Sync + Clone>: HasSource<A> + Sized + Clone + Send + Sync {
    fn sample(&self) -> A;

    fn snapshot<B, E>(&self, event: &E) -> Snapshot<A, B>
        where B: Send + Sync + Clone, E: Event<B>
    {
        Snapshot::new(self, event)
    }

    fn switch<B>(&self) -> Switch<B, A>
        where A: Behaviour<B>,
              B: Send + Sync + Clone,
    {
        Switch::new(self)
    }
}


pub struct Hold<A> {
    holder: Arc<RwLock<Holder<A>>>,
}

impl<A> Clone for Hold<A> {
    fn clone(&self) -> Hold<A> {
        Hold { holder: self.holder.clone() }
    }
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
        self.holder.write().unwrap().sample()
    }
}


pub struct Switch<A, Be> {
    switcher: Arc<RwLock<BehaviourSwitcher<A, Be>>>,
}

impl<A, Be> Clone for Switch<A, Be> {
    fn clone(&self) -> Switch<A, Be> {
        Switch { switcher: self.switcher.clone() }
    }
}

impl<A: Send + Sync + Clone, Be: Behaviour<A>> Switch<A, Be> {
    pub fn new<BBe: Behaviour<Be>>(behaviour: &BBe) -> Switch<A, Be> {
        let switch = Switch {
            switcher: Arc::new(RwLock::new(
                BehaviourSwitcher::new(
                    behaviour.sample().sample(),
                    behaviour.source().wrap_as_subject(),
                )
            ))
        };
        behaviour.source().write().unwrap().listen(switch.switcher.wrap_as_listener());
        switch
    }
}

impl<A: Send + Sync + Clone, Be: Behaviour<A>> HasSource<A> for Switch<A, Be> {
    type Source = BehaviourSwitcher<A, Be>;

    fn source(&self) -> &Arc<RwLock<BehaviourSwitcher<A, Be>>> {
        &self.switcher
    }
}

impl<A: Send + Sync + Clone, Be: Behaviour<A>> Behaviour<A> for Switch<A, Be> {
    fn sample(&self) -> A {
        self.switcher.read().unwrap().sample()
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

    #[test]
    fn switch() {
        let stream1 = Sink::<Option<i32>>::new();
        let stream2 = Sink::<Option<i32>>::new();
        let button1 = Sink::<()>::new();
        let button2 = Sink::<()>::new();
        let tv = {
            let stream1 = stream1.hold(None);
            let stream2 = stream2.hold(None);
            button1.map(move |_| stream1.clone())
                .merge(&button2.map(move |_| stream2.clone()))
        };
    }
}
