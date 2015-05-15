//! Utilities for the test suite.

use std::sync::Arc;

use signal::Signal;


/// The identity function.
pub fn id<T>(t: T) -> T { t }

/// Trace equality of two signals.
pub fn signal_eq<T>(a: &Signal<T>, b: &Signal<T>) -> Signal<bool>
    where T: PartialEq + Clone + Send + Sync + 'static
{
    lift!(|a, b| a == b, a, b)
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
    use super::signal_eq;

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
}
