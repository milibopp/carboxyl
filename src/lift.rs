//! Lifting of n-ary functions.
//!
//! A lift maps a function on values to a function on cells. Given a function of
//! type `F: Fn(A, B, …) -> R` and cells of types `Cell<A>, Cell<B>, …` the
//! `lift!` macro creates a `Cell<R>`, whose content is computed using the
//! function.
//!
//! Currently lift is only implemented for functions with up to four arguments.
//! This limitation is due to the current implementation strategy (and maybe
//! limitations of Rust's type system), but it can be increased to arbitrary but
//! finite arity if required.
//!
//! # Example
//!
//! ```
//! # #[macro_use] extern crate carboxyl;
//! # fn main() {
//! # use carboxyl::Sink;
//! let sink_a = Sink::new();
//! let sink_b = Sink::new();
//! let product = lift!(
//!     |a, b| a * b,
//!     &sink_a.stream().hold(0),
//!     &sink_b.stream().hold(0)
//! );
//! assert_eq!(product.sample(), 0);
//! sink_a.send(3);
//! sink_b.send(5);
//! assert_eq!(product.sample(), 15);
//! # }
//! ```

use std::sync::{Arc, Mutex};
use subject::{Lift2, WeakLift2Wrapper, WrapArc};
use transaction::commit;
use {Sink, Cell};


#[macro_export]
macro_rules! lift {
    ($f: expr, $a: expr)
        => ( $crate::lift::lift1($f, $a) );

    ($f: expr, $a: expr, $b: expr)
        => ( $crate::lift::lift2($f, $a, $b) );

    ($f: expr, $a: expr, $b: expr, $c: expr)
        => ( $crate::lift::lift3($f, $a, $b, $c) );

    ($f: expr, $a: expr, $b: expr, $c: expr, $d: expr)
        => ( $crate::lift::lift4($f, $a, $b, $c, $d) );
}

/// Lift a unary function.
pub fn lift1<F, A, Ret>(f: F, ca: &Cell<A>) -> Cell<Ret>
where F: Fn(A) -> Ret + Send + Sync + 'static,
      A: Clone + Send + Sync + 'static,
      Ret: Clone + Send + Sync + 'static,
{
    lift2(move |a, _| f(a), ca, &Sink::new().stream().hold(()))
}

/// Lift a binary function.
pub fn lift2<F, A, B, Ret>(f: F, ca: &Cell<A>, cb: &Cell<B>) -> Cell<Ret>
where F: Fn(A, B) -> Ret + Send + Sync + 'static,
      A: Send + Sync + Clone + 'static,
      B: Send + Sync + Clone + 'static,
      Ret: Send + Sync + Clone + 'static,
{
    commit((f, ca, cb), |(f, ca, cb)| {
        let source = Arc::new(Mutex::new(Lift2::new(
            (ca.sample_nocommit(), cb.sample_nocommit()), f, (ca.source.clone(), cb.source.clone())
        )));
        ca.source.lock().ok().expect("lift2 (ca)")
            .listen(source.wrap_as_listener());
        cb.source.lock().ok().expect("lift2 (cb)")
            .listen(WeakLift2Wrapper::boxed(&source));
        Cell { source: source.wrap_into_sampling_subject() }
    })
}

/// Lift a ternary function.
pub fn lift3<F, A, B, C, Ret>(f: F, ca: &Cell<A>, cb: &Cell<B>, cc: &Cell<C>)
    -> Cell<Ret>
where F: Fn(A, B, C) -> Ret + Send + Sync + 'static,
      A: Send + Sync + Clone + 'static,
      B: Send + Sync + Clone + 'static,
      C: Send + Sync + Clone + 'static,
      Ret: Send + Sync + Clone + 'static,
{
    lift2(move |(a, b), c| f(a, b, c), &lift2(|a, b| (a, b), ca, cb), cc)
}

/// Lift a quarternary function.
pub fn lift4<F, A, B, C, D, Ret>(f: F, ca: &Cell<A>, cb: &Cell<B>, cc: &Cell<C>, cd: &Cell<D>)
    -> Cell<Ret>
where F: Fn(A, B, C, D) -> Ret + Send + Sync + 'static,
      A: Send + Sync + Clone + 'static,
      B: Send + Sync + Clone + 'static,
      C: Send + Sync + Clone + 'static,
      D: Send + Sync + Clone + 'static,
      Ret: Send + Sync + Clone + 'static,
{
    lift2(
        move |(a, b), (c, d)| f(a, b, c, d),
        &lift2(|a, b| (a, b), ca, cb),
        &lift2(|c, d| (c, d), cc, cd)
    )
}


#[cfg(test)]
mod test {
    use Sink;

    #[test]
    fn lift1_test() {
        let sink = Sink::new();
        let lifted = lift!(|x| x + 2, &sink.stream().hold(3));
        assert_eq!(lifted.sample(), 5);
    }

    #[test]
    fn lift2_test() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let lifted = lift!(|a, b| a + b, &sink1.stream().hold(0), &sink2.stream().hold(3));
        assert_eq!(lifted.sample(), 3);
        sink1.send(1);
        assert_eq!(lifted.sample(), 4);
        sink2.send(11);
        assert_eq!(lifted.sample(), 12);
    }

    #[test]
    fn lift3_test() {
        let sink = Sink::new();
        assert_eq!(
            lift!(|x, y, z| x + 2 * y + z,
                &sink.stream().hold(5),
                &sink.stream().hold(3),
                &sink.stream().hold(-4)
            ).sample(),
            7
        );
    }

    #[test]
    fn lift4_test() {
        let sink = Sink::new();
        assert_eq!(
            lift!(|w, x, y, z| 4 * w + x + 2 * y + z,
                &sink.stream().hold(-2),
                &sink.stream().hold(5),
                &sink.stream().hold(3),
                &sink.stream().hold(-4)
            ).sample(),
            -1
        );
    }
}
