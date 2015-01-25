use std::sync::{Arc, Mutex};
use subject::{Lift2, WeakLift2Wrapper, WrapArc};
use transaction::commit;
use Cell;


/// Lift a two-argument function to a function on cells.
///
/// A lift maps a function on values to a function on cells. This particular
/// function works only with a two-argument function and effectively turns two
/// cells over types `A` and `B` into a cell over type `C`, given a function of
/// type `F: Fn(A, B) -> C` by simply applying it two the inner values of the
/// cells. So essentially `f(ba.sample(), bb.sample())` is the same as
/// `lift2(f, ba, bb).sample()`.
///
/// The following example multiplies two behaviours:
///
/// ```
/// # use carboxyl::{Sink, lift2};
/// let sink_a = Sink::<i32>::new();
/// let sink_b = Sink::<i32>::new();
/// let product = lift2(
///     |a, b| a * b,
///     &sink_a.stream().hold(0),
///     &sink_b.stream().hold(0)
/// );
/// assert_eq!(product.sample(), 0);
/// sink_a.send(3);
/// sink_b.send(5);
/// assert_eq!(product.sample(), 15);
/// ```
pub fn lift2<A, B, C, F>(f: F, ba: &Cell<A>, bb: &Cell<B>) -> Cell<C>
    where A: Send + Sync + Clone,
          B: Send + Sync + Clone,
          C: Send + Sync + Clone,
          F: Fn(A, B) -> C + Send + Sync,
{
    commit((f, ba, bb), |(f, ba, bb)| {
        let source = Arc::new(Mutex::new(Lift2::new(
            (ba.sample_nocommit(), bb.sample_nocommit()), f, (ba.source.clone(), bb.source.clone())
        )));
        ba.source.lock().ok().expect("lift2 (ba)")
            .listen(source.wrap_as_listener());
        bb.source.lock().ok().expect("lift2 (bb)")
            .listen(WeakLift2Wrapper::boxed(&source));
        Cell { source: source.wrap_into_sampling_subject() }
    })
}


#[cfg(test)]
mod test {
    use Sink;
    use super::*;

    #[test]
    fn lift2_test() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let lifted = lift2(|a, b| a + b, &sink1.stream().hold(0), &sink2.stream().hold(3));
        assert_eq!(lifted.sample(), 3);
        sink1.send(1);
        assert_eq!(lifted.sample(), 4);
        sink2.send(11);
        assert_eq!(lifted.sample(), 12);
    }

}
