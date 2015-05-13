//! Utilities for the test suite.

use signal::Signal;


/// Trace equality of two signals.
pub fn signal_eq<T>(a: &Signal<T>, b: &Signal<T>) -> Signal<bool>
    where T: PartialEq + Clone + Send + Sync + 'static
{
    lift!(|a, b| a == b, a, b)
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
