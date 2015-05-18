//! Somewhat more involved example derived from 2D drag & drop application.

#[macro_use(lift)]
extern crate carboxyl;

use carboxyl::{ Sink, Signal };


#[test]
fn drag_drop_example() {
    #[derive(Copy, Debug, Clone)]
    enum Event { Add(i32), Drag(usize, i32), Drop }
    let sink = Sink::new();
    let rects = Signal::<Vec<i32>>::cyclic(|rects| {
        let events = sink.stream();

        let spawned = rects.snapshot(&events,
            |mut rects, ev| match ev {
                Event::Add(r) => { rects.push(r); rects },
                _ => rects,
            })
            .hold(vec![300]);

        events.filter_map({
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
            .switch()
    });

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
