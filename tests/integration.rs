//! Integration tests.

#[macro_use(lift)]
extern crate carboxyl;

use carboxyl::{ Sink, Stream };


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
