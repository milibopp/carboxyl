//! Lifting of n-ary functions.
//!
//! A lift maps a function on values to a function on signals. Given a function of
//! type `F: Fn(A, B, …) -> R` and signals of types `Signal<A>, Signal<B>, …` the
//! `lift!` macro creates a `Signal<R>`, whose content is computed using the
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

use std::sync::Arc;
use signal::{ Signal, SignalFn, signal_build, signal_current, signal_source, reg_signal, sample_raw };
use transaction::commit;


#[macro_export]
macro_rules! lift {
    ($f: expr)
        => ( $crate::lift::lift0($f) );

    ($f: expr, $a: expr)
        => ( $crate::lift::lift1($f, $a) );

    ($f: expr, $a: expr, $b: expr)
        => ( $crate::lift::lift2($f, $a, $b) );

    ($f: expr, $a: expr, $b: expr, $c: expr)
        => ( $crate::lift::lift3($f, $a, $b, $c) );

    ($f: expr, $a: expr, $b: expr, $c: expr, $d: expr)
        => ( $crate::lift::lift4($f, $a, $b, $c, $d) );
}


/// Lift a 0-ary function.
pub fn lift0<A, F>(f: F) -> Signal<A>
    where F: Fn() -> A + Send + Sync + 'static
{
    commit(|| signal_build(SignalFn::from_fn(f), ()))
}


/// Lift a unary function.
pub fn lift1<A, B, F>(f: F, sa: &Signal<A>) -> Signal<B>
    where A: Send + Sync + Clone + 'static,
          B: Send + Sync + Clone + 'static,
          F: Fn(A) -> B + Send + Sync + 'static,
{
    fn make_callback<A, B, F>(f: &Arc<F>, parent: &Signal<A>) -> SignalFn<B>
        where A: Send + Sync + Clone + 'static,
              B: Send + Sync + Clone + 'static,
              F: Fn(A) -> B + Send + Sync + 'static,
    {
        let pclone = parent.clone();
        let f = f.clone();
        match *signal_current(&parent).read().unwrap().future() {
            SignalFn::Const(ref a) => SignalFn::Const(f(a.clone())),
            SignalFn::Func(_) => SignalFn::from_fn(move || f(sample_raw(&pclone))),
        }
    }

    commit(|| {
        let f = Arc::new(f);
        let signal = signal_build(make_callback(&f, &sa), ());
        let sa_clone = sa.clone();
        reg_signal(&mut signal_source(&sa).write().unwrap(), &signal,
            move |_| make_callback(&f, &sa_clone));
        signal
    })
}


/// Lift a binary function.
pub fn lift2<A, B, C, F>(f: F, sa: &Signal<A>, sb: &Signal<B>) -> Signal<C>
    where A: Send + Sync + Clone + 'static,
          B: Send + Sync + Clone + 'static,
          C: Send + Sync + Clone + 'static,
          F: Fn(A, B) -> C + Send + Sync + 'static,
{
    fn make_callback<A, B, C, F>(f: &Arc<F>, sa: &Signal<A>, sb: &Signal<B>) -> SignalFn<C>
        where A: Send + Sync + Clone + 'static,
              B: Send + Sync + Clone + 'static,
              C: Send + Sync + Clone + 'static,
              F: Fn(A, B) -> C + Send + Sync + 'static,
    {
        use signal::SignalFn::{ Const, Func };
        let sa_clone = sa.clone();
        let sb_clone = sb.clone();
        let f = f.clone();
        match (
            signal_current(&sa).read().unwrap().future(),
            signal_current(&sb).read().unwrap().future(),
        ) {
            (&Const(ref a), &Const(ref b)) => Const(f(a.clone(), b.clone())),
            (&Const(ref a), &Func(_)) => {
                let a = a.clone();
                SignalFn::from_fn(move || f(a.clone(), sample_raw(&sb_clone)))
            },
            (&Func(_), &Const(ref b)) => {
                let b = b.clone();
                SignalFn::from_fn(move || f(sample_raw(&sa_clone), b.clone()))
            },
            (&Func(_), &Func(_)) => SignalFn::from_fn(
                move || f(sample_raw(&sa_clone), sample_raw(&sb_clone))
            ),
        }
    }

    commit(move || {
        let f = Arc::new(f);
        let signal = signal_build(make_callback(&f, &sa, &sb), ());
        reg_signal(&mut signal_source(&sa).write().unwrap(), &signal, {
            let sa_clone = sa.clone();
            let sb_clone = sb.clone();
            let f = f.clone();
            move |_| make_callback(&f, &sa_clone, &sb_clone)
        });
        reg_signal(&mut signal_source(&sb).write().unwrap(), &signal, {
            let sa_clone = sa.clone();
            let sb_clone = sb.clone();
            move |_| make_callback(&f, &sa_clone, &sb_clone)
        });
        signal
    })
}

/// Lift a ternary function.
pub fn lift3<F, A, B, C, Ret>(f: F, ca: &Signal<A>, cb: &Signal<B>, cc: &Signal<C>)
    -> Signal<Ret>
where F: Fn(A, B, C) -> Ret + Send + Sync + 'static,
      A: Send + Sync + Clone + 'static,
      B: Send + Sync + Clone + 'static,
      C: Send + Sync + Clone + 'static,
      Ret: Send + Sync + Clone + 'static,
{
    lift2(move |(a, b), c| f(a, b, c), &lift2(|a, b| (a, b), ca, cb), cc)
}

/// Lift a quarternary function.
pub fn lift4<F, A, B, C, D, Ret>(f: F, ca: &Signal<A>, cb: &Signal<B>, cc: &Signal<C>, cd: &Signal<D>)
    -> Signal<Ret>
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
    use stream::Sink;
    use signal::Signal;

    #[test]
    fn lift0() {
        let signal = lift!(|| 3);
        assert_eq!(signal.sample(), 3);
    }

    #[test]
    fn lift1() {
        let sig2 = lift!(|n| n + 2, &Signal::new(3));
        assert_eq!(sig2.sample(), 5);
    }

    #[test]
    fn lift2() {
        let sink1 = Sink::new();
        let sink2 = Sink::new();
        let lifted = lift!(|a, b| a + b, &sink1.stream().hold(0),
            &sink2.stream().hold(3));
        assert_eq!(lifted.sample(), 3);
        sink1.send(1);
        assert_eq!(lifted.sample(), 4);
        sink2.send(11);
        assert_eq!(lifted.sample(), 12);
    }

    #[test]
    fn lift2_identical() {
        let sig = Signal::new(16);
        let sig2 = lift!(|a, b| a + b, &sig, &sig);
        assert_eq!(sig2.sample(), 32);
    }

    #[test]
    fn lift3() {
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
    fn lift4() {
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

    #[test]
    fn lift0_equal_within_transaction() {
        use rand::random;
        // Generate a completely random signal
        let rnd = lift!(random::<i64>);
        // Make a tuple with itself
        let gather = lift!(|a, b| (a, b), &rnd, &rnd);
        // Both components should be equal
        let (a, b) = gather.sample();
        assert_eq!(a, b);
    }
}
