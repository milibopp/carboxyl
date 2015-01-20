//! A trivial global lock transaction system.
//!
//! At the moment, this is really just a global static mutex, that needs to be
//! locked, to ensure the atomicity of a transaction.

use std::sync::{StaticMutex, MUTEX_INIT};


/// The global transaction lock.
static TRANSACTION_MUTEX: StaticMutex = MUTEX_INIT;


/// Commit a transaction.
///
/// Acquires the global lock and then executes the closure.
pub fn commit<A, B, F: FnOnce(A) -> B>(args: A, transaction: F) -> B {
    let _lock = TRANSACTION_MUTEX.lock().ok()
        .expect("global transaction mutex poisoned");
    transaction(args)
}
