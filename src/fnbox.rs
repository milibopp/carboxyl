//! FnBox replacement

/// Specialized replacement for unstable FnBox from stdlib
pub trait FnBox {
    /// Call a boxed closure
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<Self>) {
        self();
    }
}
