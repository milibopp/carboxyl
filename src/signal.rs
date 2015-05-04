//! Continuous time signals

use std::sync::{ Arc, Mutex };
use std::ops::Deref;
use source::{ Source, with_weak, CallbackError };
use stream::{ self, BoxClone, Stream };
use transaction::{ commit, register_callback };
use pending::Pending;


pub enum SignalFn<A> {
    Const(A),
    Func(Box<Fn() -> A + Send + Sync + 'static>),
}

impl<A> SignalFn<A> {
    pub fn from_fn<F: Fn() -> A + Send + Sync + 'static>(f: F) -> SignalFn<A> {
        SignalFn::Func(Box::new(f))
    }
}

impl<A: Clone> SignalFn<A> {
    pub fn call(&self) -> A {
        match *self {
            SignalFn::Const(ref a) => a.clone(),
            SignalFn::Func(ref f) => f(),
        }
    }
}


/// Helper function to register callback handlers related to signal construction.
pub fn reg_signal<A, B, F>(parent_source: &mut Source<A>, signal: &Signal<B>, handler: F)
    where A: Send + Sync + 'static,
          B: Send + Sync + 'static,
          F: Fn(A) -> SignalFn<B> + Send + Sync + 'static,
{
    let weak_source = signal.source.downgrade();
    let weak_current = signal.current.downgrade();
    parent_source.register(move |a|
        weak_current.upgrade().map(|cur| register_callback(
            move || { let _ = cur.lock().map(|mut cur| cur.update()); }))
            .ok_or(CallbackError::Disappeared)
        .and(with_weak(&weak_current, |cur| cur.queue(handler(a))))
        .and(with_weak(&weak_source, |src| src.send(())))
    );
}


/// External helper function to build a signal.
pub fn signal_build<A, K>(func: SignalFn<A>, keep_alive: K) -> Signal<A>
        where K: Send + Sync + Clone + 'static
{
    Signal::build(func, keep_alive)
}

/// External accessor to current state of a signal.
pub fn signal_current<A>(signal: &Signal<A>) -> &Arc<Mutex<Pending<SignalFn<A>>>> {
    &signal.current
}

/// External accessor to signal source.
pub fn signal_source<A>(signal: &Signal<A>) -> &Arc<Mutex<Source<()>>> {
    &signal.source
}


/// A continuous signal.
pub struct Signal<A> {
    current: Arc<Mutex<Pending<SignalFn<A>>>>,
    source: Arc<Mutex<Source<()>>>,
    #[allow(dead_code)]
    keep_alive: Box<BoxClone>,
}

impl<A> Clone for Signal<A> {
    fn clone(&self) -> Signal<A> {
        Signal {
            current: self.current.clone(),
            source: self.source.clone(),
            keep_alive: self.keep_alive.box_clone(),
        }
    }
}

impl<A> Signal<A> {
    fn build<K>(func: SignalFn<A>, keep_alive: K) -> Signal<A>
        where K: Send + Sync + Clone + 'static
    {
        Signal {
            current: Arc::new(Mutex::new(Pending::new(func))),
            source: Arc::new(Mutex::new(Source::new())),
            keep_alive: Box::new(keep_alive),
        }
    }
}

impl<A: Clone> Signal<A> {
    /// Create a constant signal.
    pub fn new(a: A) -> Signal<A> {
        Signal::build(SignalFn::Const(a), ())
    }

    /// Sample the current value of a signal.
    pub fn sample(&self) -> A {
        commit((), |_| self.current.lock().unwrap().call())
    }
}

impl<A: Clone + Send + Sync + 'static> Signal<A> {
    /// stub
    pub fn snapshot<B>(&self, stream: &Stream<B>) -> Stream<(A, B)>
        where B: Clone + Send + Sync + 'static,
    {
        stream::snapshot(self, stream)
    }
}

impl<A: Clone + Send + Sync + 'static> Signal<Signal<A>> {
    /// stub
    pub fn switch(&self) -> Signal<A> {
        fn make_callback<A>(parent: &Signal<Signal<A>>) -> SignalFn<A>
            where A: Send + Clone + Sync + 'static,
        {
            // TODO: use information on inner value
            let current_signal = parent.current.clone();
            SignalFn::from_fn(move ||
                current_signal.lock().unwrap().call().sample()
            )
        }
        commit((), |_| {
            let signal = Signal::build(make_callback(self), ());
            let parent = self.clone();
            reg_signal(&mut self.source.lock().unwrap(), &signal,
                move |_| make_callback(&parent));
            signal
        })
    }
}


/// stub
pub struct SignalCycle<A> {
    signal: Signal<A>,
}

impl<A: Send + Sync + Clone + 'static> SignalCycle<A> {
    /// stub
    pub fn new() -> SignalCycle<A> {
        const ERR: &'static str = "sampled on forward-declaration of signal";
        SignalCycle { signal: Signal::build(SignalFn::from_fn(|| panic!(ERR)), ()) }
    }

    /// stub
    pub fn define(self, definition: Signal<A>) -> Signal<A> {
        /// Generate a callback from the signal definition's current value.
        fn make_callback<A>(current_def: &Arc<Mutex<Pending<SignalFn<A>>>>) -> SignalFn<A>
            where A: Send + Sync + Clone + 'static
        {
            match *current_def.lock().unwrap().future() {
                SignalFn::Const(ref a) => SignalFn::Const(a.clone()),
                SignalFn::Func(_) => SignalFn::from_fn({
                    let sig = current_def.downgrade();
                    move || {
                        let strong = sig.upgrade().unwrap();
                        let ret = strong.lock().unwrap().call();
                        ret
                    }
                }),
            }
        }
        commit((), move |_| {
            *self.signal.current.lock().unwrap() = Pending::new(make_callback(&definition.current));
            let weak_parent = definition.current.downgrade();
            reg_signal(&mut definition.source.lock().unwrap(), &self.signal,
                move |_| make_callback(&weak_parent.upgrade().unwrap()));
            Signal { keep_alive: Box::new(definition), ..self.signal }
        })
    }
}

impl<A> Deref for SignalCycle<A> {
    type Target = Signal<A>;
    fn deref(&self) -> &Signal<A> { &self.signal }
}


/// Hold a stream as a signal.
pub fn hold<A>(initial: A, stream: &Stream<A>) -> Signal<A>
    where A: Send + Sync + 'static,
{
    commit((), |_| {
        let signal = Signal::build(SignalFn::Const(initial), stream.clone());
        reg_signal(&mut stream::source(&stream).lock().unwrap(), &signal, SignalFn::Const);
        signal
    })
}




#[cfg(test)]
mod test {
    use ::stream::Sink;
    use ::signal::{ self, Signal, SignalCycle };
    use ::lift::lift1;

    #[test]
    fn clone() {
        let b = Signal::new(3);
        assert_eq!(b.clone().sample(), 3);
    }

    #[test]
    fn hold() {
        let sink = Sink::new();
        let signal = sink.stream().hold(3);
        assert_eq!(signal.sample(), 3);
        sink.send(4);
        assert_eq!(signal.sample(), 4);
    }

    #[test]
    fn hold_implicit_stream() {
        let sink = Sink::new();
        let signal = signal::hold(0, &sink.stream().map(|n| 2 * n));
        assert_eq!(signal.sample(), 0);
        sink.send(4);
        assert_eq!(signal.sample(), 8);
    }

    #[test]
    fn snapshot() {
        let sink1: Sink<i32> = Sink::new();
        let sink2: Sink<f64> = Sink::new();
        let mut snap_events = sink1.stream().hold(1).snapshot(&sink2.stream().map(|x| x + 3.0)).events();
        sink2.send(4.0);
        assert_eq!(snap_events.next(), Some((1, 7.0)));
    }

    #[test]
    fn snapshot_2() {
        let ev1 = Sink::new();
        let beh1 = ev1.stream().hold(5);
        let ev2 = Sink::new();
        let snap = beh1.snapshot(&ev2.stream());
        let mut events = snap.events();
        ev2.send(4);
        assert_eq!(events.next(), Some((5, 4)));
        ev1.send(-2);
        ev2.send(6);
        assert_eq!(events.next(), Some((-2, 6)));
    }

    #[test]
    fn switch() {
        let stream1 = Sink::<Option<i32>>::new();
        let stream2 = Sink::<Option<i32>>::new();
        let button1 = Sink::<()>::new();
        let button2 = Sink::<()>::new();
        let output = {
            let stream1 = stream1.stream().hold(Some(-1));
            let stream2 = stream2.stream().hold(Some(-2));
            button1.stream()
                .map(move |_| stream1.clone())
                .merge(&button2.stream()
                    .map(move |_| stream2.clone())
                )
                .hold(Sink::new().stream().hold(Some(-3)))
                .switch()
        };
        let fn_output = lift1(|x| x.map_or(0, |x| 2*x), &output);
        assert_eq!(output.sample(), Some(-3));
        assert_eq!(fn_output.sample(), -6);
        button1.send(());
        assert_eq!(output.sample(), Some(-1));
        button2.send(());
        assert_eq!(output.sample(), Some(-2));
        button1.send(());
        stream1.send(Some(6));
        assert_eq!(output.sample(), Some(6));
        assert_eq!(fn_output.sample(), 12);
        stream1.send(Some(12));
        assert_eq!(output.sample(), Some(12));
        assert_eq!(fn_output.sample(), 24);
    }

    #[test]
    fn cyclic_snapshot_accum() {
        let sink = Sink::new();
        let stream = sink.stream();
        let accum = SignalCycle::new();
        let def = accum.snapshot(&stream).map(|(a, s)| a + s).hold(0);
        let accum = accum.define(def);
        assert_eq!(accum.sample(), 0);
        sink.send(3);
        assert_eq!(accum.sample(), 3);
        sink.send(7);
        assert_eq!(accum.sample(), 10);
        sink.send(-21);
        assert_eq!(accum.sample(), -11);
    }

    #[test]
    fn snapshot_order_standard() {
        let sink = Sink::new();
        let cell = sink.stream().hold(0);
        let mut events = cell.snapshot(&sink.stream()).events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }

    #[test]
    fn snapshot_lift_order_standard() {
        let sink = Sink::new();
        let cell = sink.stream().hold(0);
        let mut events = lift1(|x| x, &cell)
            .snapshot(&sink.stream())
            .events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }

    #[test]
    fn snapshot_order_alternative() {
        let sink = Sink::new();
        // Invert the "natural" order of the registry by declaring the stream before
        // the cell, which are both used by the snapshot.
        let first = sink.stream().map(|x| x);
        let cell = sink.stream().hold(0);
        let mut events = cell.snapshot(&first).events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }
}
