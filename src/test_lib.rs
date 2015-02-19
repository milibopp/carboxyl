//! High-level unit tests of the primitives

use test::Bencher;
use super::*;

#[test]
fn sink() {
    let sink = Sink::new();
    let mut events = sink.stream().events();
    sink.send(1);
    sink.send(2);
    assert_eq!(events.next(), Some(1));
    assert_eq!(events.next(), Some(2));
}

#[test]
fn map() {
    let sink = Sink::new();
    let triple = sink.stream().map(|x| 3 * x);
    let mut events = triple.events();
    sink.send(1);
    assert_eq!(events.next(), Some(3));
}

#[test]
fn filter() {
    let sink = Sink::new();
    let small = sink.stream().filter_just();
    let mut events = small.events();
    sink.send(None);
    sink.send(Some(9));
    assert_eq!(events.next(), Some(9));
}

#[test]
fn merge() {
    let sink1 = Sink::new();
    let sink2 = Sink::new();
    let mut events = sink1.stream().merge(&sink2.stream()).events();
    sink1.send(12);
    sink2.send(9);
    assert_eq!(events.next(), Some(12));
    assert_eq!(events.next(), Some(9));
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
    let sink: Sink<i32> = Sink::new();
    let chain = sink.stream().map(|x| x / 2).filter(|&x| x < 3);
    let mut events = chain.events();
    sink.send(7);
    sink.send(4);
    assert_eq!(events.next(), Some(2));
}

#[test]
fn chain_2() {
    let sink1: Sink<i32> = Sink::new();
    let sink2: Sink<i32> = Sink::new();
    let mut events = sink1.stream().map(|x| x + 4)
        .merge(
            &sink2.stream()
            .filter_map(|x| if x < 4 { Some(x) } else { None })
            .map(|x| x * 5))
        .events();
    sink1.send(12);
    sink2.send(3);
    assert_eq!(events.next(), Some(16));
    assert_eq!(events.next(), Some(15));
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
fn updates() {
    let sink = Sink::new();
    let mut events = sink.stream().hold(0).updates().events();
    sink.send(4);
    assert_eq!(events.next(), Some(4));
    sink.send(-14);
    assert_eq!(events.next(), Some(-14));
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
    let fn_output = lift!(|x| x.map_or(0, |x| 2*x), &output);
    assert_eq!(output.sample(), Some(-3));
    assert_eq!(fn_output.sample(), -6);
    button1.send(());
    assert_eq!(output.sample(), Some(-1));
    button2.send(());
    assert_eq!(output.sample(), Some(-2));
    button1.send(());
    stream1.send(Some(6));
    assert_eq!(output.sample(), Some(6));
    assert_eq!(fn_output.sample(), 12);
    stream1.send(Some(12));
    assert_eq!(output.sample(), Some(12));
    assert_eq!(fn_output.sample(), 24);
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
    let mut events = sink.stream().events();
    sink.send_async(1);
    assert_eq!(events.next(), Some(1));
}

#[test]
fn sink_feed() {
    let sink = Sink::new();
    let events = sink.stream().events();
    sink.feed(0..10);
    for (n, m) in events.take(10).enumerate() {
        assert_eq!(n as i32, m);
    }
}

#[test]
fn sink_feed_async() {
    let sink = Sink::new();
    let events = sink.stream().events();
    sink.feed_async(0..10);
    for (n, m) in events.take(10).enumerate() {
        assert_eq!(n as i32, m);
    }
}

#[bench]
fn bench_chain(b: &mut Bencher) {
    let sink: Sink<i32> = Sink::new();
    let _ = sink.stream()
        .map(|x| x + 4)
        .filter(|&x| x < 4)
        .merge(&sink.stream().map(|x| x * 5))
        .hold(15);
    b.iter(|| sink.send(-5));
}

#[test]
fn snapshot_order_standard() {
    let sink = Sink::new();
    let cell = sink.stream().hold(0);
    let mut events = cell.snapshot(&sink.stream()).events();
    sink.send(1);
    assert_eq!(events.next(), Some((0, 1)));
}

#[test]
fn snapshot_order_alternative() {
    let sink = Sink::new();
    // Invert the "natural" order of the registry by declaring the stream before
    // the cell, which are both used by the snapshot.
    let first = sink.stream().map(|x| x);
    let cell = sink.stream().hold(0);
    let mut events = cell.snapshot(&first).events();
    sink.send(1);
    assert_eq!(events.next(), Some((0, 1)));
}

#[test]
fn cyclic_snapshot_accum() {
    let sink = Sink::new();
    let stream = sink.stream();
    let accum = CellCycle::new(0);
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
fn nested_transactions() {
    let sink = Sink::new();
    let stream = sink.stream();
    let switched = stream
        .map(|k| Stream::never().hold(k))
        .hold(Stream::never().hold(0))
        .switch();
    assert_eq!(switched.sample(), 0);
    sink.send(3);
    assert_eq!(switched.sample(), 3);
    sink.send(5);
    assert_eq!(switched.sample(), 5);
}

#[test]
fn drag_drop_example() {
    #[derive(Copy, Debug, Clone)]
    enum Event { Add(i32), Drag(usize, i32), Drop }
    let sink = Sink::new();
    let rects = CellCycle::<Vec<i32>>::new(vec![100]);
    let events = sink.stream();

    let spawned = rects.snapshot(&events)
        .map(|(mut rects, ev)| match ev {
            Event::Add(r) => { rects.push(r); rects },
            _ => rects,
        })
        .hold(vec![300]);

    let new_rects = events.filter_map({
        let spawned = spawned.clone();
        move |ev| match ev {
            Event::Drag(idx, pos) => {
            Some(lift!(
                move |mut rects| {
                    rects[idx] += pos;
                    rects
                },
                &spawned
            ))},
            Event::Drop => Some(spawned.clone()),
            _ => None,
        }})
        .hold(spawned.clone())
        .switch();
    let rects = rects.define(new_rects.clone());

    assert_eq!(rects.sample(), vec![300]);
    sink.send(Event::Add(61));
    assert_eq!(rects.sample(), vec![300, 61]);
    sink.send(Event::Add(66));
    assert_eq!(rects.sample(), vec![300, 61, 66]);
    sink.send(Event::Drop);
    assert_eq!(rects.sample(), vec![300, 61, 66]);
    sink.send(Event::Drag(1, 10));
    assert_eq!(rects.sample(), vec![300, 71, 66]);
}
