//! A trivial global lock transaction system.
//!
//! At the moment, this is really just a global static mutex, that needs to be
//! locked, to ensure the atomicity of a transaction.

use std::sync::Mutex;
use std::cell::RefCell;
use std::boxed::FnBox;


/// The global transaction lock.
///
/// TODO: revert this to use a static mutex, as soon as that is stabilized in
/// the standard library.
lazy_static! {
    static ref TRANSACTION_MUTEX: Mutex<()> = Mutex::new(());
}

/// Registry for callbacks to be executed at the end of a transaction.
thread_local!(
    static CURRENT_TRANSACTION: RefCell<Option<Transaction>> =
        RefCell::new(None)
);


/// A callback.
type Callback = Box<FnBox() + 'static>;


/// A transaction.
pub struct Transaction {
    intermediate: Vec<Callback>,
    finalizers: Vec<Callback>,
}

impl Transaction {
    /// Create a new transaction
    fn new() -> Transaction {
        Transaction {
            intermediate: vec![],
            finalizers: vec![],
        }
    }

    /// Add a callback that will be called, when the transaction is done
    /// except for finalizers.
    pub fn later<F: FnOnce() + 'static>(&mut self, callback: F) {
        self.intermediate.push(Box::new(callback));
    }

    /// Add a finalizing callback. This should not have far reaching
    /// side-effects, and in particular not commit by itself. Typical operations
    /// for a finalizer are executing queued state updates.
    pub fn end<F: FnOnce() + 'static>(&mut self, callback: F) {
        self.finalizers.push(Box::new(callback));
    }

    /// Advance transactions by moving out intermediate stage callbacks.
    fn advance(&mut self) -> Vec<Callback> {
        use std::mem;
        let mut intermediate = vec![];
        mem::swap(&mut intermediate, &mut self.intermediate);
        intermediate
    }

    /// Finalize the transaction
    fn finalize(self) {
        for finalizer in self.finalizers {
            finalizer.call_box(());
        }
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
        None => Some(TRANSACTION_MUTEX.lock().ok()
                .expect("global transaction mutex poisoned")
        ),
        Some(_) => None,
    };
    // Perform the main body of the transaction
    let result = body();
    // Advance the transaction as long as necessary
    loop {
        let callbacks = with_current(Transaction::advance);
        if callbacks.is_empty() { break }
        for callback in callbacks {
            callback.call_box(());
        }
    }
    // Call all finalizers and drop the transaction
    CURRENT_TRANSACTION.with(|current|
        mem::swap(&mut prev, &mut current.borrow_mut())
    );
    prev.unwrap().finalize();
    // Return
    result
}


/// Register a callback during a transaction.
pub fn with_current<A, F: FnOnce(&mut Transaction) -> A>(action: F) -> A {
    CURRENT_TRANSACTION.with(|current|
        match &mut *current.borrow_mut() {
            &mut Some(ref mut trans) => action(trans),
            _ => panic!("there is no active transaction to register a callback"),
        }
    )
}

pub fn later<F: FnOnce() + 'static>(action: F) {
    with_current(|c| c.later(action))
}

pub fn end<F: FnOnce() + 'static>(action: F) {
    with_current(|c| c.end(action))
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

        // Set up a ref-counted value
        let v = Arc::new(Mutex::new(3));
        // Spawn a couple of scoped threads performing atomic operations on it
        let guards: Vec<_> = (0..3)
            .map(|_| {
                let v = v.clone();
                thread::spawn(move || commit(move || {
                    // Acquire locks independently, s.t. commit atomicity does
                    // not rely on the local locks here
                    *v.lock().unwrap() *= 2;
                    // …and sleep for a bit
                    thread::sleep_ms(1);
                    *v.lock().unwrap() -= 1;
                }))
            })
            .collect();
        // Rejoin with all guards
        for guard in guards { guard.join().ok().expect("thread failed"); }
        // Check result
        assert_eq!(&*v.lock().unwrap(), &17);
    }
}
