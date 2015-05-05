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


/// A continuous signal that changes over time.
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

    /// Sample the current value of the signal.
    pub fn sample(&self) -> A {
        commit((), |_| self.current.lock().unwrap().call())
    }
}

impl<A: Clone + Send + Sync + 'static> Signal<A> {
    /// Combine the signal with a stream in a snapshot.
    ///
    /// `snapshot` creates a new stream given a signal and a stream. Whenever
    /// the input stream fires an event, the output stream fires a pair of the
    /// signal's current value and that event.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink1: Sink<i32> = Sink::new();
    /// let sink2: Sink<f64> = Sink::new();
    /// let snapshot = sink1.stream().hold(1).snapshot(&sink2.stream());
    /// let mut events = snapshot.events();
    ///
    /// // Updating its signal does not cause the snapshot to fire
    /// sink1.send(4);
    ///
    /// // However sending an event down the stream does
    /// sink2.send(3.0);
    /// assert_eq!(events.next(), Some((4, 3.0)));
    /// ```
    pub fn snapshot<B>(&self, stream: &Stream<B>) -> Stream<(A, B)>
        where B: Clone + Send + Sync + 'static,
    {
        stream::snapshot(self, stream)
    }
}

impl<A: Clone + Send + Sync + 'static> Signal<Signal<A>> {
    /// Switch between signals.
    ///
    /// This transforms a `Signal<Signal<A>>` into a `Signal<A>`. The nested
    /// signal can be thought of as a representation of a switch between different
    /// input signals, that allows one to change the structure of the dependency
    /// graph at run-time. `switch` provides a way to access the inner value of
    /// the currently active signal.
    ///
    /// The following example demonstrates how to use this to switch between two
    /// input signals based on a `Button` event stream:
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// // Button type
    /// #[derive(Clone, Show)]
    /// enum Button { A, B };
    ///
    /// // The input sinks
    /// let sink_a = Sink::<i32>::new();
    /// let sink_b = Sink::<i32>::new();
    ///
    /// // The button sink
    /// let sink_button = Sink::<Button>::new();
    ///
    /// // Create the output
    /// let output = {
    ///
    ///     // Hold input sinks in a signal with some initials
    ///     let channel_a = sink_a.stream().hold(1);
    ///     let channel_b = sink_b.stream().hold(2);
    ///
    ///     // A trivial default channel used before any button event
    ///     let default_channel = Sink::new().stream().hold(0);
    ///
    ///     // Map button to the channel signals, hold with the default channel as
    ///     // initial value and switch between the signals
    ///     sink_button
    ///         .stream()
    ///         .map(move |b| match b {
    ///             Button::A => channel_a.clone(),
    ///             Button::B => channel_b.clone(),
    ///         })
    ///         .hold(default_channel)
    ///         .switch()
    /// };
    ///
    /// // In the beginning, output will come from the default channel
    /// assert_eq!(output.sample(), 0);
    ///
    /// // Let's switch to channel A
    /// sink_button.send(Button::A);
    /// assert_eq!(output.sample(), 1);
    ///
    /// // And to channel B
    /// sink_button.send(Button::B);
    /// assert_eq!(output.sample(), 2);
    ///
    /// // The channels can change, too, of course
    /// for k in 4..13 {
    ///     sink_b.send(k);
    ///     assert_eq!(output.sample(), k);
    /// }
    /// sink_button.send(Button::A);
    /// for k in 21..77 {
    ///     sink_a.send(k);
    ///     assert_eq!(output.sample(), k);
    /// }
    /// ```
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


/// Forward declaration of a signal to create value loops.
///
/// This pattern is useful to implement accumulators, counters and other loops
/// that depend on the sampling behaviour of a signal before a transaction.
pub struct SignalCycle<A> {
    signal: Signal<A>,
}

impl<A: Send + Sync + Clone + 'static> SignalCycle<A> {
    /// Forward-declare a new signal.
    pub fn new() -> SignalCycle<A> {
        const ERR: &'static str = "sampled on forward-declaration of signal";
        SignalCycle { signal: Signal::build(SignalFn::from_fn(|| panic!(ERR)), ()) }
    }

    /// Provide the signal with a definition.
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
        let signal = sink.stream().hold(0);
        let mut events = signal.snapshot(&sink.stream()).events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }

    #[test]
    fn snapshot_lift_order_standard() {
        let sink = Sink::new();
        let signal = sink.stream().hold(0);
        let mut events = lift1(|x| x, &signal)
            .snapshot(&sink.stream())
            .events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }

    #[test]
    fn snapshot_order_alternative() {
        let sink = Sink::new();
        // Invert the "natural" order of the registry by declaring the stream before
        // the signal, which are both used by the snapshot.
        let first = sink.stream().map(|x| x);
        let signal = sink.stream().hold(0);
        let mut events = signal.snapshot(&first).events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }
}
