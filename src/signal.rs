//! Continuous time signals

use std::sync::{ Arc, Mutex, RwLock };
use std::ops::Deref;
use std::fmt;
#[cfg(test)]
use quickcheck::{ Arbitrary, Gen };

use source::{ Source, with_weak, CallbackError };
use stream::{ self, BoxClone, Stream };
use transaction::{ commit, later };
use pending::Pending;
use lift;
#[cfg(test)]
use testing::ArcFn;


/// A functional signal. Caches its return value during a transaction.
pub struct FuncSignal<A> {
    func: Box<Fn() -> A + Send + Sync + 'static>,
    cache: Arc<Mutex<Option<A>>>,
}

impl<A> FuncSignal<A> {
    pub fn new<F: Fn() -> A + Send + Sync + 'static>(f: F) -> FuncSignal<A> {
        FuncSignal {
            func: Box::new(f),
            cache: Arc::new(Mutex::new(None)),
        }
    }
}

impl<A: Clone + 'static> FuncSignal<A> {
    /// Call the function or fetch the cached value if present.
    pub fn call(&self) -> A {
        let mut cached = self.cache.lock().unwrap();
        match &mut *cached {
            &mut Some(ref value) => value.clone(),
            cached => {
                // Register callback to reset cache at the end of the transaction
                let cache = self.cache.clone();
                later(move || {
                    let mut live = cache.lock().unwrap();
                    *live = None;
                });
                // Calculate & cache value
                let value = (self.func)();
                *cached = Some(value.clone());
                value
            },
        }
    }
}


pub enum SignalFn<A> {
    Const(A),
    Func(FuncSignal<A>),
}

impl<A> SignalFn<A> {
    pub fn from_fn<F: Fn() -> A + Send + Sync + 'static>(f: F) -> SignalFn<A> {
        SignalFn::Func(FuncSignal::new(f))
    }
}

impl<A: Clone + 'static> SignalFn<A> {
    pub fn call(&self) -> A {
        match *self {
            SignalFn::Const(ref a) => a.clone(),
            SignalFn::Func(ref f) => f.call(),
        }
    }
}


/// Helper function to register callback handlers related to signal construction.
pub fn reg_signal<A, B, F>(parent_source: &mut Source<A>, signal: &Signal<B>, handler: F)
    where A: Send + Sync + 'static,
          B: Send + Sync + 'static,
          F: Fn(A) -> SignalFn<B> + Send + Sync + 'static,
{
    let weak_source = Arc::downgrade(&signal.source);
    let weak_current = Arc::downgrade(&signal.current);
    parent_source.register(move |a|
        weak_current.upgrade().map(|cur| later(
            move || { let _ = cur.write().map(|mut cur| cur.update()); }))
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
pub fn signal_current<A>(signal: &Signal<A>) -> &Arc<RwLock<Pending<SignalFn<A>>>> {
    &signal.current
}

/// External accessor to signal source.
pub fn signal_source<A>(signal: &Signal<A>) -> &Arc<RwLock<Source<()>>> {
    &signal.source
}

/// Sample the value of the signal without committing it as a transaction.
pub fn sample_raw<A: Clone + 'static>(signal: &Signal<A>) -> A {
    signal.current.read().unwrap().call()
}


/// A continuous signal that changes over time.
///
/// Signals can be thought of as values that change over time. They have both a
/// continuous and a discrete component. This means that their current value is
/// defined by a function that can be called at any time. That function is only
/// evaluated on-demand, when the signal's current value is sampled. (This is
/// also called pull semantics in the literature on FRP.)
///
/// In addition, the current function used to sample a signal may change
/// discretely in reaction to some event. For instance, it is possible to create
/// a signal from an event stream, by holding the last event occurence as the
/// current value of the stream.
///
/// # Algebraic laws
///
/// Signals come with some primitive methods to compose them with each other and
/// with streams. Some of these primitives give the signals an algebraic
/// structure.
///
/// ## Functor
///
/// Signals form a functor under unary lifting. Thus, the following laws hold:
///
/// - Preservation of identity: `lift!(|x| x, &a) == a`,
/// - Function composition: `lift!(|x| g(f(x)), &a) == lift!(g, &lift!(f, &a))`.
///
/// ## Applicative functor
///
/// By extension, using the notion of a signal of a function, signals also
/// become an [applicative][ghc-applicative] using `Signal::new` as `pure` and
/// `|sf, sa| lift!(|f, a| f(a), &sf, &sa)` as `<*>`.
///
/// *TODO: Expand on this and replace the Haskell reference.*
///
/// [ghc-applicative]: https://downloads.haskell.org/~ghc/latest/docs/html/libraries/base/Control-Applicative.html
pub struct Signal<A> {
    current: Arc<RwLock<Pending<SignalFn<A>>>>,
    source: Arc<RwLock<Source<()>>>,
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
            current: Arc::new(RwLock::new(Pending::new(func))),
            source: Arc::new(RwLock::new(Source::new())),
            keep_alive: Box::new(keep_alive),
        }
    }
}

impl<A: Clone + 'static> Signal<A> {
    /// Create a constant signal.
    pub fn new(a: A) -> Signal<A> {
        Signal::build(SignalFn::Const(a), ())
    }

    /// Sample the current value of the signal.
    pub fn sample(&self) -> A {
        commit(|| sample_raw(self))
    }
}

impl<A: Clone + Send + Sync + 'static> Signal<A> {
    /// Create a signal with a cyclic definition.
    ///
    /// The closure gets an undefined forward-declaration of a signal. It is
    /// supposed to return a self-referential definition of the same signal.
    ///
    /// Sampling the forward-declared signal, before it is properly defined,
    /// will cause a run-time panic.
    ///
    /// This pattern is useful to implement accumulators, counters and other
    /// loops that depend on the sampling behaviour of a signal before a
    /// transaction.
    pub fn cyclic<F>(def: F) -> Signal<A>
        where F: FnOnce(&Signal<A>) -> Signal<A>
    {
        commit(|| {
            let cycle = SignalCycle::new();
            let finished = def(&cycle);
            cycle.define(finished)
        })
    }

    /// Combine the signal with a stream in a snapshot.
    ///
    /// `snapshot` creates a new stream given a signal and a stream. Whenever
    /// the input stream fires an event, the output stream fires an event
    /// created from the signal's current value and that event using the
    /// supplied function.
    ///
    /// ```
    /// # use carboxyl::Sink;
    /// let sink1: Sink<i32> = Sink::new();
    /// let sink2: Sink<f64> = Sink::new();
    /// let mut events = sink1.stream().hold(1)
    ///     .snapshot(&sink2.stream(), |a, b| (a, b))
    ///     .events();
    ///
    /// // Updating its signal does not cause the snapshot to fire
    /// sink1.send(4);
    ///
    /// // However sending an event down the stream does
    /// sink2.send(3.0);
    /// assert_eq!(events.next(), Some((4, 3.0)));
    /// ```
    pub fn snapshot<B, C, F>(&self, stream: &Stream<B>, f: F) -> Stream<C>
        where B: Clone + Send + Sync + 'static,
              C: Clone + Send + Sync + 'static,
              F: Fn(A, B) -> C + Send + Sync + 'static,
    {
        stream::snapshot(self, stream, f)
    }

    /// Map a signal using a function.
    ///
    /// Same as `lift!` with a single argument signal.
    pub fn map<B, F>(&self, function: F) -> Signal<B>
        where B: Clone + Send + Sync + 'static,
              F: Fn(A) -> B + Send + Sync + 'static
    {
        lift::lift1(function, self)
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
    /// #[derive(Clone)]
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
                sample_raw(&current_signal.read().unwrap().call())
            )
        }
        commit(|| {
            let signal = Signal::build(make_callback(self), ());
            let parent = self.clone();
            reg_signal(&mut self.source.write().unwrap(), &signal,
                move |_| make_callback(&parent));
            signal
        })
    }
}

#[cfg(test)]
impl<A, B> Signal<ArcFn<A, B>>
    where A: Clone + Send + Sync + 'static,
          B: Clone + Send + Sync + 'static,
{
    /// Applicative functionality. Applies a signal of function to a signal of
    /// its argument.
    fn apply(&self, signal: &Signal<A>) -> Signal<B> {
        lift::lift2(|f, a| f(a), self, signal)
    }
}

#[cfg(test)]
impl<A: Arbitrary + Sync + Clone + 'static> Arbitrary for Signal<A> {
    fn arbitrary<G: Gen>(g: &mut G) -> Signal<A> {
        let values = Vec::<A>::arbitrary(g);
        if values.is_empty() {
            Signal::new(Arbitrary::arbitrary(g))
        } else {
            let n = Mutex::new(0);
            lift::lift0(move || {
                let mut n = n.lock().unwrap();
                *n += 1;
                if *n >= values.len() { *n = 0 }
                values[*n].clone()
            })
        }
    }
}

impl<A: fmt::Debug + Clone + 'static> fmt::Debug for Signal<A> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        commit(|| match **self.current.read().unwrap() {
            SignalFn::Const(ref a) =>
                fmt.debug_struct("Signal::const").field("value", &a).finish(),
            SignalFn::Func(ref f) =>
                fmt.debug_struct("Signal::fn").field("current", &f.call()).finish(),
        })
    }
}


/// Forward declaration of a signal to create value loops.
struct SignalCycle<A> {
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
        fn make_callback<A>(current_def: &Arc<RwLock<Pending<SignalFn<A>>>>) -> SignalFn<A>
            where A: Send + Sync + Clone + 'static
        {
            match *current_def.read().unwrap().future() {
                SignalFn::Const(ref a) => SignalFn::Const(a.clone()),
                SignalFn::Func(_) => SignalFn::from_fn({
                    let sig = Arc::downgrade(&current_def);
                    move || {
                        let strong = sig.upgrade().unwrap();
                        let ret = strong.read().unwrap().call();
                        ret
                    }
                }),
            }
        }
        commit(move || {
            *self.signal.current.write().unwrap() = Pending::new(make_callback(&definition.current));
            let weak_parent = Arc::downgrade(&definition.current);
            reg_signal(&mut definition.source.write().unwrap(), &self.signal,
                move |_| make_callback(&weak_parent.upgrade().unwrap()));
            Signal { keep_alive: Box::new(definition), ..self.signal }
        })
    }
}

impl<A> Deref for SignalCycle<A> {
    type Target = Signal<A>;
    fn deref(&self) -> &Signal<A> { &self.signal }
}


/// Same as `Stream::hold`.
pub fn hold<A>(initial: A, stream: &Stream<A>) -> Signal<A>
    where A: Send + Sync + 'static,
{
    commit(|| {
        let signal = Signal::build(SignalFn::Const(initial), stream.clone());
        reg_signal(&mut stream::source(&stream).write().unwrap(), &signal, SignalFn::Const);
        signal
    })
}


#[cfg(test)]
mod test {
    use quickcheck::quickcheck;

    use ::stream::Sink;
    use ::signal::{ self, Signal, SignalCycle };
    use ::lift::lift1;
    use ::testing::{ ArcFn, signal_eq, id, pure_fn, partial_comp };

    #[test]
    fn functor_identity() {
        fn check(signal: Signal<i32>) -> bool {
            let eq = signal_eq(&signal, &lift1(id, &signal));
            (0..10).all(|_| eq.sample())
        }
        quickcheck(check as fn(Signal<i32>) -> bool);
    }

    #[test]
    fn functor_composition() {
        fn check(signal: Signal<i32>) -> bool {
            fn f(n: i32) -> i32 { 3 * n }
            fn g(n: i32) -> i32 { n + 2 }
            let eq = signal_eq(
                &lift1(|n| f(g(n)), &signal),
                &lift1(f, &lift1(g, &signal))
            );
            (0..10).all(|_| eq.sample())
        }
        quickcheck(check as fn(Signal<i32>) -> bool);
    }

    #[test]
    fn applicative_identity() {
        fn check(signal: Signal<i32>) -> bool {
            let eq = signal_eq(&pure_fn(id).apply(&signal), &signal);
            (0..10).all(|_| eq.sample())
        }
        quickcheck(check as fn(Signal<i32>) -> bool);
    }

    #[test]
    fn applicative_composition() {
        fn check(signal: Signal<i32>) -> bool {
            fn f(n: i32) -> i32 { n * 4 }
            fn g(n: i32) -> i32 { n - 3 }
            let u = pure_fn(f);
            let v = pure_fn(g);
            let eq = signal_eq(
                &pure_fn(partial_comp).apply(&u).apply(&v).apply(&signal),
                &u.apply(&v.apply(&signal))
            );
            (0..10).all(|_| eq.sample())
        }
        quickcheck(check as fn(Signal<i32>) -> bool);
    }

    #[test]
    fn applicative_homomorphism() {
        fn check(x: i32) -> bool {
            fn f(x: i32) -> i32 { x * (-5) }
            let eq = signal_eq(
                &pure_fn(f).apply(&Signal::new(x)),
                &Signal::new(f(x))
            );
            (0..10).all(|_| eq.sample())
        }
        quickcheck(check as fn(i32) -> bool);
    }

    #[test]
    fn applicative_interchange() {
        fn check(x: i32) -> bool {
            fn f(x: i32) -> i32 { x * 2 - 7 }
            let u = pure_fn(f);
            let eq = signal_eq(
                &u.apply(&Signal::new(x)),
                &pure_fn(move |f: ArcFn<i32, i32>| f(x)).apply(&u)
            );
            (0..10).all(|_| eq.sample())
        }
        quickcheck(check as fn(i32) -> bool);
    }

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
        let mut snap_events = sink1.stream().hold(1)
            .snapshot(&sink2.stream().map(|x| x + 3.0), |a, b| (a, b))
            .events();
        sink2.send(4.0);
        assert_eq!(snap_events.next(), Some((1, 7.0)));
    }

    #[test]
    fn snapshot_2() {
        let ev1 = Sink::new();
        let beh1 = ev1.stream().hold(5);
        let ev2 = Sink::new();
        let snap = beh1.snapshot(&ev2.stream(), |a, b| (a, b));
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
        let def = accum.snapshot(&stream, |a, s| a + s).hold(0);
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
        let mut events = signal
            .snapshot(&sink.stream(), |a, b| (a, b))
            .events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }

    #[test]
    fn snapshot_lift_order_standard() {
        let sink = Sink::new();
        let signal = sink.stream().hold(0);
        let mut events = lift1(|x| x, &signal)
            .snapshot(&sink.stream(), |a, b| (a, b))
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
        let mut events = signal.snapshot(&first, |a, b| (a, b)).events();
        sink.send(1);
        assert_eq!(events.next(), Some((0, 1)));
    }

    #[test]
    fn cyclic_signal_intermediate() {
        let sink = Sink::new();
        let stream = sink.stream();
        let mut snap = None;
        let sum = Signal::cyclic(|a| {
            let my_snap = a.snapshot(&stream, |a, e| e + a);
            snap = Some(my_snap.clone());
            my_snap.hold(0)
        });
        let snap = snap.unwrap();
        let mut events = snap.events();

        sink.send(3);
        assert_eq!(sum.sample(), 3);
        assert_eq!(events.next(), Some(3));
    }
}
