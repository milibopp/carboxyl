//! Event sources and callbacks.
//!
//! This is a light-weight implementation of the observer pattern. Subjects are
//! modelled as the `Source` type and observers as boxed closures.

use std::sync::{ RwLock, Weak };

/// An error that can occur with a weakly referenced callback.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CallbackError {
    Disappeared,
    Poisoned,
}

/// Shorthand for common callback results.
pub type CallbackResult<T=()> = Result<T, CallbackError>;

/// A boxed callback.
type Callback<A> = Box<dyn FnMut(A) -> CallbackResult + Send + Sync + 'static>;


/// Perform some callback on a weak reference to a mutex and handle errors
/// gracefully.
pub fn with_weak<T, U, F: FnOnce(&mut T) -> U>(weak: &Weak<RwLock<T>>, f: F) -> CallbackResult<U> {
    weak.upgrade()
        .ok_or(CallbackError::Disappeared)
        .and_then(|mutex| mutex.write()
            .map(|mut t| f(&mut t))
            .map_err(|_| CallbackError::Poisoned)
        )
}


/// An event source.
pub struct Source<A> {
    callbacks: Vec<Callback<A>>,
}

impl<A> Source<A> {
    /// Create a new source.
    pub fn new() -> Source<A> {
        Source { callbacks: vec![] }
    }

    /// Register a callback. The callback will be a mutable closure that takes
    /// an event and must return a result. To unsubscribe from further events,
    /// the callback has to return an error.
    pub fn register<F>(&mut self, callback: F)
        where F: FnMut(A) -> CallbackResult + Send + Sync + 'static
    {
        self.callbacks.push(Box::new(callback));
    }
}

impl<A: Send + Sync + Clone + 'static> Source<A> {
    /// Make the source send an event to all its observers.
    pub fn send(&mut self, a: A) {
        use std::mem;
        let mut new_callbacks = vec!();
        mem::swap(&mut new_callbacks, &mut self.callbacks);
        let n = new_callbacks.len();
        let mut iter = new_callbacks.into_iter();
        for _ in 1..n {
            let mut callback = iter.next().unwrap();
            if let Ok(_) = callback(a.clone()) {
                self.callbacks.push(callback);
            }
        }
        // process the last element without cloning
        if let Some(mut callback) = iter.next() {
            if let Ok(_) = callback(a) {
                self.callbacks.push(callback);
            }
        }
    }
}


#[cfg(test)]
mod test {
    use std::sync::{ Arc, RwLock };
    use std::thread;
    use super::*;

    #[test]
    fn with_weak_no_error() {
        let a = Arc::new(RwLock::new(3));
        let weak = Arc::downgrade(&a);
        assert_eq!(with_weak(&weak, |a| { *a = 4; }), Ok(()));
        assert_eq!(*a.read().unwrap(), 4);
    }

    #[test]
    fn with_weak_disappeared() {
        let weak = Arc::downgrade(&Arc::new(RwLock::new(3)));
        assert_eq!(with_weak(&weak, |_| ()), Err(CallbackError::Disappeared));
    }

    #[test]
    fn with_weak_poisoned() {
        let a = Arc::new(RwLock::new(3));
        let a2 = a.clone();
        let weak = Arc::downgrade(&a);
        let _ = thread::spawn(move || {
            let _g = a2.write().unwrap();
            panic!();
        }).join();
        assert_eq!(with_weak(&weak, |_| ()), Err(CallbackError::Poisoned));
    }

    #[test]
    fn source_register_and_send() {
        let mut src = Source::new();
        let a = Arc::new(RwLock::new(3));
        {
            let a = a.clone();
            src.register(move |x| {
                *a.write().unwrap() = x;
                Ok(())
            });
        }
        assert_eq!(src.callbacks.len(), 1);
        src.send(4);
        assert_eq!(*a.read().unwrap(), 4);
    }

    #[test]
    fn source_unregister() {
        let mut src = Source::new();
        src.register(|_| Err(CallbackError::Disappeared));
        assert_eq!(src.callbacks.len(), 1);
        src.send(());
        assert_eq!(src.callbacks.len(), 0);
    }
}
