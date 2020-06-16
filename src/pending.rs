//! Pending wrapper

use std::ops::Deref;

/// A pending value. This is a wrapper type that allows one to queue one new
/// value without actually overwriting the old value. Later the most recently
/// queued value can be updated.
pub struct Pending<T> {
    current: T,
    update: Option<T>,
}

impl<T> Pending<T> {
    /// Create a new pending value.
    pub fn new(t: T) -> Pending<T> {
        Pending {
            current: t,
            update: None,
        }
    }

    /// Put an item in the queue. Ignores any previously queued items.
    pub fn queue(&mut self, new: T) {
        self.update = Some(new);
    }

    /// Updates any update pending.
    pub fn update(&mut self) {
        if let Some(t) = self.update.take() {
            self.current = t;
        }
    }

    /// Get the future value.
    pub fn future(&self) -> &T {
        self.update.as_ref().unwrap_or(&self.current)
    }
}

impl<T> Deref for Pending<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.current
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_derefs_identical() {
        assert_eq!(*Pending::new(3), 3);
    }

    #[test]
    fn queue_does_not_affect_deref() {
        let mut p = Pending::new(2);
        p.queue(4);
        assert_eq!(*p, 2);
    }

    #[test]
    fn new_future_identical() {
        assert_eq!(*Pending::new(5).future(), 5);
    }

    #[test]
    fn queue_affects_future() {
        let mut p = Pending::new(10);
        p.queue(6);
        assert_eq!(*p.future(), 6);
    }

    #[test]
    fn updated_deref() {
        let mut p = Pending::new(-2);
        p.queue(2);
        p.update();
        assert_eq!(*p, 2);
    }

    #[test]
    fn updated_future() {
        let mut p = Pending::new(-7);
        p.queue(0);
        p.update();
        assert_eq!(*p.future(), 0);
    }
}
