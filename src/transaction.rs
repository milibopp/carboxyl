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
    static FINAL_CALLBACKS: RefCell<Vec<Box<Fn() + 'static>>>
        = RefCell::new(Vec::new())
);


/// Commit a transaction.
///
/// Acquires the global lock and then executes the closure.
pub fn commit<A, B, F: FnOnce(A) -> B>(args: A, transaction: F) -> B {
    // Acquire lock
    let _lock = TRANSACTION_MUTEX.lock().ok()
        .expect("global transaction mutex poisoned");
    // Make sure there are no stale callbacks
    FINAL_CALLBACKS.with(|store| {
        if store.borrow().len() > 0 {
            store.borrow_mut().clear();
        }
    });
    // Perform the transaction
    let result = transaction(args);
    // Call all finalizers
    FINAL_CALLBACKS.with(|store| {
        for callback in store.borrow_mut().drain() {
            callback();
        }
    });
    // Return
    result
}


/// Register a callback during a transaction.
pub fn register_callback<F: Fn() + 'static>(callback: F) {
    FINAL_CALLBACKS.with(move |store| {
        store.borrow_mut().push(Box::new(callback))
    });
}
