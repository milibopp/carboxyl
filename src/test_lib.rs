//! High-level unit tests of the primitives

use test::Bencher;
use super::*;

#[test]
fn sink() {
    let sink = Sink::new();
    let mut iter = sink.stream().iter();
    sink.send(1);
    sink.send(2);
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(2));
}

#[test]
fn map() {
    let sink = Sink::new();
    let triple = sink.stream().map(|x| 3 * x);
    let mut iter = triple.iter();
    sink.send(1);
    assert_eq!(iter.next(), Some(3));
}

#[test]
fn filter() {
    let sink = Sink::new();
    let small = sink.stream().filter();
    let mut iter = small.iter();
    sink.send(None);
    sink.send(Some(9));
    assert_eq!(iter.next(), Some(9));
}

#[test]
fn merge() {
    let sink1 = Sink::new();
    let sink2 = Sink::new();
    let mut iter = sink1.stream().merge(&sink2.stream()).iter();
    sink1.send(12);
    sink2.send(9);
    assert_eq!(iter.next(), Some(12));
    assert_eq!(iter.next(), Some(9));
}

#[test]
fn hold() {
    let ea = Sink::new();
    let ba = ea.stream().hold(3);
    assert_eq!(ba.sample(), 3);
    ea.send(4);
    assert_eq!(ba.sample(), 4);
}

#[test]
fn chain_1() {
    let sink = Sink::<i32>::new();
    let chain = sink.stream().map(|x| x / 2).filter_with(|&x| x < 3);
    let mut iter = chain.iter();
    sink.send(7);
    sink.send(4);
    assert_eq!(iter.next(), Some(2));
}

#[test]
fn chain_2() {
    let sink1: Sink<i32> = Sink::new();
    let sink2: Sink<i32> = Sink::new();
    let mut iter = sink1.stream().map(|x| x + 4)
        .merge(
            &sink2.stream()
            .map(|x| if x < 4 { Some(x) } else { None })
            .filter().map(|x| x * 5))
        .iter();
    sink1.send(12);
    sink2.send(3);
    assert_eq!(iter.next(), Some(16));
    assert_eq!(iter.next(), Some(15));
}

#[test]
fn snapshot() {
    let sink1: Sink<i32> = Sink::new();
    let sink2: Sink<f64> = Sink::new();
    let mut snap_iter = sink1.stream().hold(1).snapshot(&sink2.stream().map(|x| x + 3.0)).iter();
    sink2.send(4.0);
    assert_eq!(snap_iter.next(), Some((1, 7.0)));
}

#[test]
fn snapshot_2() {
    let ev1 = Sink::new();
    let beh1 = ev1.stream().hold(5);
    let ev2 = Sink::new();
    let snap = beh1.snapshot(&ev2.stream());
    let mut iter = snap.iter();
    ev2.send(4);
    assert_eq!(iter.next(), Some((5, 4)));
    ev1.send(-2);
    ev2.send(6);
    assert_eq!(iter.next(), Some((-2, 6)));
}

#[test]
fn updates() {
    let sink = Sink::new();
    let mut iter = sink.stream().hold(0).updates().iter();
    sink.send(4);
    assert_eq!(iter.next(), Some(4));
    sink.send(-14);
    assert_eq!(iter.next(), Some(-14));
}

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
    assert_eq!(output.sample(), Some(-3));
    button1.send(());
    assert_eq!(output.sample(), Some(-1));
    button2.send(());
    assert_eq!(output.sample(), Some(-2));
    button1.send(());
    stream1.send(Some(6));
    assert_eq!(output.sample(), Some(6));
    stream1.send(Some(12));
    assert_eq!(output.sample(), Some(12));
}

#[test]
fn clone() {
    let sink = Sink::new();
    let b = sink.stream().hold(1);
    sink.clone().send(3);
    assert_eq!(b.sample(), 3);
}

#[test]
fn move_closure() {
    let sink = Sink::<i32>::new();
    let x = 3;
    sink.stream().map(move |y| y + x);
}

#[test]
fn sink_send_async() {
    let sink = Sink::new();
    let mut iter = sink.stream().iter();
    sink.send_async(1);
    assert_eq!(iter.next(), Some(1));
}

#[test]
fn sink_feed() {
    let sink = Sink::new();
    let iter = sink.stream().iter();
    sink.feed(0..10);
    for (n, m) in iter.take(10).enumerate() {
        assert_eq!(n, m);
    }
}

#[test]
fn sink_feed_async() {
    let sink = Sink::new();
    let iter = sink.stream().iter();
    sink.feed_async(0..10);
    for (n, m) in iter.take(10).enumerate() {
        assert_eq!(n, m);
    }
}

#[bench]
fn bench_chain(b: &mut Bencher) {
    let sink: Sink<i32> = Sink::new();
    let _ = sink.stream()
        .map(|x| x + 4)
        .filter_with(|&x| x < 4)
        .merge(&sink.stream().map(|x| x * 5))
        .hold(15);
    b.iter(|| sink.send(-5));
}

#[test]
fn snapshot_order_standard() {
    let sink = Sink::new();
    let cell = sink.stream().hold(0);
    let mut iter = cell.snapshot(&sink.stream()).iter();
    sink.send(1);
    assert_eq!(iter.next(), Some((0, 1)));
}

#[test]
fn snapshot_order_alternative() {
    let sink = Sink::new();
    // Invert the "natural" order of the registry by declaring the stream before
    // the cell, which are both used by the snapshot.
    let first = sink.stream().map(|x| x);
    let cell = sink.stream().hold(0);
    let mut iter = cell.snapshot(&first).iter();
    sink.send(1);
    assert_eq!(iter.next(), Some((0, 1)));
}

#[test]
fn cyclic_snapshot_accum() {
    let sink = Sink::<i32>::new();
    let stream = sink.stream();
    let accum = Cell::<i32>::cyclic(0, |accum|
        accum.snapshot(&stream)
            .map(|(a, s)| a + s)
    );
    assert_eq!(accum.sample(), 0);
    sink.send(3);
    assert_eq!(accum.sample(), 3);
    sink.send(7);
    assert_eq!(accum.sample(), 10);
    sink.send(-21);
    assert_eq!(accum.sample(), -11);
}
