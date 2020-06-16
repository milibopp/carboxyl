//! Utilities for the test suite.

use std::sync::Arc;
use std::fmt::Debug;
use rand::random;

use lift::{ lift0, lift1 };
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

/// Error of stream equivalence.
#[derive(Copy, Clone, PartialEq, Debug)]
enum EquivError<T> {
    Mismatch(T, T),
    OnlyOne(T),
}

/// Simple pairing function.
fn pair<T, U>(t: T, u: U) -> (T, U) {
    (t, u)
}

/// Trace equality of two streams.
pub fn stream_eq<T>(a: &Stream<T>, b: &Stream<T>) -> Signal<Result<(), String>>
    where T: PartialEq + Clone + Send + Sync + 'static + Debug,
{
    use self::EquivError::*;
    let tagger = lift0(random::<u64>);
    let result = tagger.snapshot(a, pair)
        .merge(&tagger.snapshot(b, pair))
        .map(|x| Err(OnlyOne(x)))
        .coalesce(|a, b| match (a, b) {
            (Err(OnlyOne(a)), Err(OnlyOne(b))) =>
                if &a == &b { Ok(()) }
                else { Err(Mismatch(a, b)) },
            (Err(Mismatch(a, b)), Err(OnlyOne(_))) => Err(Mismatch(a, b)),
            (Ok(()), Err(OnlyOne(a))) => Err(OnlyOne(a)),
            _ => unreachable!(),
        })
        .fold(Ok(()), |state, a| state.and(a));
    lift1(|r| r.map_err(|e| format!("{:?}", e)), &result)
}

/// A boxed function.
pub type ArcFn<A, B> = Arc<dyn Fn(A) -> B + Send + Sync + 'static>;

/// Wrap a function into a constant signal of that function.
pub fn pure_fn<A, B, F>(f: F) -> Signal<ArcFn<A, B>>
    where A: 'static,
          B: 'static,
          F: Fn(A) -> B + Send + Sync + 'static,
{
    Signal::new(make_fn(f))
}

/// Box a function to erase its type.
pub fn make_fn<A, B, F>(f: F) -> ArcFn<A, B>
    where A: 'static,
          B: 'static,
          F: Fn(A) -> B + Send + Sync + 'static,
{
    Arc::new(f)
}

/// Function composition on boxed functions.
pub fn comp<A, B, C>(f: ArcFn<B, C>, g: ArcFn<A, B>) -> ArcFn<A, C>
    where A: 'static, B: 'static, C: 'static
{
    make_fn(move |a| f(g(a)))
}

/// Partially applied function composition.
pub fn partial_comp<A, B, C>(f: ArcFn<B, C>) -> ArcFn<ArcFn<A, B>, ArcFn<A, C>>
    where A: 'static, B: 'static, C: 'static
{
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
        assert_eq!(eq.sample(), Ok(()));
    }

    #[test]
    fn stream_eq_same_sink() {
        let sink = Sink::new();
        let eq = stream_eq(&sink.stream(), &sink.stream());
        for n in 0..20 { sink.send(n); }
        assert_eq!(eq.sample(), Ok(()));
    }

    #[test]
    fn stream_eq_algebraic() {
        fn f(n: i32) -> i32 { n + 3 }
        fn g(n: i32) -> i32 { 2 * n }
        fn h(n: i32) -> i32 { f(g(n)) }

        let sink = Sink::new();
        let eq = stream_eq(&sink.stream().map(g).map(f), &sink.stream().map(h));
        for n in 0..20 { sink.send(n); }
        assert_eq!(eq.sample(), Ok(()));
    }
}
