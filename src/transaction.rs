//! A trivial global lock transaction system.
//!
//! At the moment, this is really just a global static mutex, that needs to be
//! locked, to ensure the atomicity of a transaction.

use std::sync::{StaticMutex, MUTEX_INIT};
use std::cell::RefCell;


/// The global transaction lock.
static TRANSACTION_MUTEX: StaticMutex = MUTEX_INIT;


/// Registry for callbacks to be executed at the end of a transaction.
thread_local!(
    static CURRENT_TRANSACTION: RefCell<Option<Transaction>> =
        RefCell::new(None)
);


/// A transaction
pub struct Transaction {
    callbacks: Vec<Box<Fn() + 'static>>,
}

impl Transaction {
    /// Create a new transaction
    pub fn new() -> Transaction {
        Transaction { callbacks: Vec::new() }
    }

    /// Add a callback
    pub fn register<F: Fn() + 'static>(&mut self, callback: F) {
        self.callbacks.push(Box::new(callback));
    }

    /// Finalize the transaction
    pub fn finalize(self) {
        for callback in self.callbacks { callback(); }
    }
}

/// Commit a transaction.
///
/// If the thread is not running any transactions currently, the global lock is
/// acquired. Otherwise a new transaction begins, since given the interface of
/// this module it is safely assumed that the lock is already held.
pub fn commit<A, B, F: FnOnce(A) -> B>(args: A, transaction: F) -> B {
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
                    .expect("global transaction mutex poisoned")),
        Some(_) => None,
    };
    // Perform the transaction
    let result = transaction(args);
    // Call all finalizers and drop the transaction
    CURRENT_TRANSACTION.with(move |current| {
        mem::swap(&mut prev, &mut current.borrow_mut());
        prev.unwrap().finalize();
    });
    // Return
    result
}


/// Register a callback during a transaction.
pub fn register_callback<F: Fn() + 'static>(callback: F) {
    CURRENT_TRANSACTION.with(move |current|
        match &mut *current.borrow_mut() {
            &mut Some(ref mut trans) => trans.register(callback),
            _ => panic!("cannot do stuff, meh… :( "),
        }
    );
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn commit_single() {
        let mut v = 3;
        commit((), |_| v += 5);
        assert_eq!(v, 8);
    }

    #[test]
    fn commit_nested() {
        let mut v = 3;
        commit((), |_| {
            commit((), |_| v *= 2);
            v += 4;
        });
        assert_eq!(v, 10);
    }

    #[test]
    fn commits_parallel() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::old_io::timer::sleep;
        use std::time::duration::Duration;

        // Set up a ref-counted value
        let v = Arc::new(Mutex::new(3));
        // Spawn a couple of scoped threads performing atomic operations on it
        let guards: Vec<_> = (0..3)
            .map(|_| {
                let v = v.clone();
                thread::spawn(move || commit((), move |_| {
                    // Acquire locks independently, s.t. commit atomicity does
                    // not rely on the local locks here
                    *v.lock().unwrap() *= 2;
                    // …and sleep for a bit
                    sleep(Duration::milliseconds(1));
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
