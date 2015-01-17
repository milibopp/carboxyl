#![feature(unboxed_closures)]
#![allow(unstable)]
#![warn(missing_docs)]

pub use primitives::{Stream, Cell, Sink, lift2};

mod subject;
mod primitives;
