#![feature(unboxed_closures)]
#![allow(unstable)]

pub use primitives::{Event, Behaviour, Sink, lift2};

mod subject;
mod primitives;
