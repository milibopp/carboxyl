//! A trivial global lock transaction system.
//!
//! At the moment, this is really just a global static mutex, that needs to be
//! locked, to ensure the atomicity of a transaction.

use crate::fnbox::FnBox;
use lazy_static::lazy_static;
use std::cell::RefCell;
use std::sync::Mutex;

lazy_static! {
    /// The global transaction lock.
    ///
    /// TODO: revert this to use a static mutex, as soon as that is stabilized in
    /// the standard library.
    static ref TRANSACTION_MUTEX: Mutex<()> = Mutex::new(());
}

thread_local!(
    /// Registry for callbacks to be executed at the end of a transaction.
    static CURRENT_TRANSACTION: RefCell<Option<Transaction>> =
        RefCell::new(None)
);

/// A callback.
type Callback = Box<dyn FnBox + 'static>;

/// A transaction.
#[derive(Default)]
pub struct Transaction {
    finalizers: Vec<Callback>,
}

impl Transaction {
    /// Create a new transaction
    fn new() -> Transaction {
        Transaction { finalizers: vec![] }
    }

    /// Add a finalizing callback. This should not have far reaching
    /// side-effects, and in particular not commit by itself. Typical operations
    /// for a finalizer are executing queued state updates.
    pub fn later<F: FnOnce() + 'static>(&mut self, callback: F) {
        self.finalizers.push(Box::new(callback));
    }

    /// Advance transactions by moving out intermediate stage callbacks.
    fn finalizers(&mut self) -> Vec<Callback> {
        use std::mem;
        let mut finalizers = vec![];
        mem::swap(&mut finalizers, &mut self.finalizers);
        finalizers
    }
}

/// Commit a transaction.
///
/// If the thread is not running any transactions currently, the global lock is
/// acquired. Otherwise a new transaction begins, since given the interface of
/// this module it is safely assumed that the lock is already held.
pub fn commit<A, F: FnOnce() -> A>(body: F) -> A {
    use std::mem;
    // Begin a new transaction
    let mut prev = CURRENT_TRANSACTION.with(|current| {
        let mut prev = Some(Transaction::new());
        mem::swap(&mut prev, &mut current.borrow_mut());
        prev
    });
    // Acquire global lock if necessary
    let _lock = match prev {
        None => Some(
            TRANSACTION_MUTEX
                .lock()
                .expect("global transaction mutex poisoned"),
        ),
        Some(_) => None,
    };
    // Perform the main body of the transaction
    let result = body();

    // if there was a previous transaction, move all the finalizers
    // there, otherwise run them here
    match prev {
        Some(ref mut trans) => with_current(|cur| trans.finalizers.append(&mut cur.finalizers)),
        None => loop {
            let callbacks = with_current(Transaction::finalizers);
            if callbacks.is_empty() {
                break;
            }
            for callback in callbacks {
                callback.call_box();
            }
        },
    }

    // Drop the transaction
    CURRENT_TRANSACTION.with(|current| mem::swap(&mut prev, &mut current.borrow_mut()));

    // Return
    result
}

/// Register a callback during a transaction.
pub fn with_current<A, F: FnOnce(&mut Transaction) -> A>(action: F) -> A {
    CURRENT_TRANSACTION.with(|current| match *current.borrow_mut() {
        Some(ref mut trans) => action(trans),
        _ => panic!("there is no active transaction to register a callback"),
    })
}

pub fn later<F: FnOnce() + 'static>(action: F) {
    with_current(|c| c.later(action))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn commit_single() {
        let mut v = 3;
        commit(|| v += 5);
        assert_eq!(v, 8);
    }

    #[test]
    fn commit_nested() {
        let mut v = 3;
        commit(|| {
            commit(|| v *= 2);
            v += 4;
        });
        assert_eq!(v, 10);
    }

    #[test]
    fn commits_parallel() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        // Set up a ref-counted value
        let v = Arc::new(Mutex::new(3));
        // Spawn a couple of scoped threads performing atomic operations on it
        let guards: Vec<_> = (0..3)
            .map(|_| {
                let v = v.clone();
                thread::spawn(move || {
                    commit(move || {
                        // Acquire locks independently, s.t. commit atomicity does
                        // not rely on the local locks here
                        *v.lock().unwrap() *= 2;
                        // …and sleep for a bit
                        thread::sleep(Duration::from_millis(1));
                        *v.lock().unwrap() -= 1;
                    })
                })
            })
            .collect();
        // Rejoin with all guards
        for guard in guards {
            guard.join().ok().expect("thread failed");
        }
        // Check result
        assert_eq!(&*v.lock().unwrap(), &17);
    }
}
