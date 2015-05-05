//! Thread-safe read-only smart pointer.

use std::sync::{ Arc, RwLock, RwLockReadGuard };
use std::ops::Deref;

/// Guards read-access into a read-only pointer.
pub struct ReadOnlyGuard<'a, T: 'a> {
    guard: RwLockReadGuard<'a, T>,
}

impl<'a, T> Deref for ReadOnlyGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { &self.guard }
}

/// A thread-safe read-only smart pointer.
pub struct ReadOnly<T> {
    ptr: Arc<RwLock<T>>,
}

impl<T> Clone for ReadOnly<T> {
    fn clone(&self) -> ReadOnly<T> {
        ReadOnly { ptr: self.ptr.clone() }
    }
}

/// Create a new read-only pointer.
pub fn create<T>(ptr: Arc<RwLock<T>>) -> ReadOnly<T> { ReadOnly { ptr: ptr } }

impl<T> ReadOnly<T> {
    /// Gain read-access to the stored value.
    ///
    /// In case, the underlying data structure has been poisoned, it returns
    /// `None`.
    pub fn read(&self) -> Option<ReadOnlyGuard<T>> {
        self.ptr.read().ok().map(|g| ReadOnlyGuard { guard: g })
    }
}
