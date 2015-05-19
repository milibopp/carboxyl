//! Utilities for the test suite.

use std::sync::Arc;
use std::fmt::Debug;
use rand::random;

use lift::lift0;
use signal::Signal;
use stream::Stream;


/// The identity function.
pub fn id<T>(t: T) -> T { t }

/// Trace equality of two signals.
pub fn signal_eq<T>(a: &Signal<T>, b: &Signal<T>) -> Signal<bool>
    where T: PartialEq + Clone + Send + Sync + 'static
{
    lift!(|a, b| a == b, a, b)
}

/// Trace equality of two streams.
pub fn stream_eq<T>(a: &Stream<T>, b: &Stream<T>) -> Signal<bool>
    where T: PartialEq + Clone + Send + Sync + 'static,
{
    fn tag<T>(t: u64, a: T) -> Option<(u64, T)> {
        Some((t, a))
    }

    let tagger = lift0(random::<u64>);
    tagger.snapshot(a, tag)
        .merge(&tagger.snapshot(b, tag))
        .coalesce(|a, b| match (a, b) {
            (Some(a), Some(b)) => {
                if &a == &b { Some(a) } else { None }
            },
            _ => None,
        })
        .scan(true, |state, a| state && a.is_some())
}

/// A boxed function.
pub type ArcFn<A, B> = Arc<Box<Fn(A) -> B + Send + Sync + 'static>>;

/// Wrap a function into a constant signal of that function.
pub fn pure_fn<A, B, F: Fn(A) -> B + Send + Sync + 'static>(f: F) -> Signal<ArcFn<A, B>> {
    Signal::new(make_fn(f))
}

/// Box a function to erase its type.
pub fn make_fn<A, B, F: Fn(A) -> B + Send + Sync + 'static>(f: F) -> ArcFn<A, B> {
    Arc::new(Box::new(f))
}

/// Function composition on boxed functions.
pub fn comp<A, B, C>(f: ArcFn<B, C>, g: ArcFn<A, B>) -> ArcFn<A, C> {
    make_fn(move |a| f(g(a)))
}

/// Partially applied function composition.
pub fn partial_comp<A, B, C>(f: ArcFn<B, C>) -> ArcFn<ArcFn<A, B>, ArcFn<A, C>> {
    make_fn(move |g| comp(f.clone(), g))
}


/// Self-tests.
mod test {
    use signal::Signal;
    use stream::{ Stream, Sink };
    use super::{ stream_eq, signal_eq };

    #[test]
    fn signal_eq_const() {
        let a = Signal::new(3);
        let eq = signal_eq(&a, &a);
        assert!(eq.sample());
    }

    #[test]
    fn signal_eq_piecewise_const() {
        let sink = Sink::new();
        let sig = sink.stream().hold(4);
        let eq = signal_eq(&sig, &sig);
        for &a in &[1, 2, 3] {
            sink.send(a);
            assert!(eq.sample());
        }
    }

    #[test]
    fn signal_eq_algebraic() {
        let eq = signal_eq(&Stream::never().hold(3), &Signal::new(3));
        assert!(eq.sample());
    }

    #[test]
    fn stream_eq_never() {
        let eq = stream_eq(&Stream::<()>::never(), &Stream::never());
        assert!(eq.sample());
    }

    #[test]
    fn stream_eq_same_sink() {
        let sink = Sink::new();
        let eq = stream_eq(&sink.stream(), &sink.stream());
        for n in 0..20 { sink.send(n); }
        assert!(eq.sample());
    }

    #[test]
    fn stream_eq_algebraic() {
        fn f(n: i32) -> i32 { n + 3 }
        fn g(n: i32) -> i32 { 2 * n }
        fn h(n: i32) -> i32 { f(g(n)) }

        let sink = Sink::new();
        let eq = stream_eq(&sink.stream().map(g).map(f), &sink.stream().map(h));
        for n in 0..20 { sink.send(n); }
        assert!(eq.sample());
    }
}
