//! Helper module for coalesce

use std::sync::{Arc, Mutex, RwLock, Weak};

use super::Stream;
use crate::source::{with_weak, CallbackError, CallbackResult, Source};
use crate::transaction::later;

fn update_value<T, F>(mutex: &Mutex<Option<T>>, a: T, f: F)
where
    T: Clone + Send + Sync + 'static,
    F: Fn(T, T) -> T + Send + Sync,
{
    let mut inner = mutex.lock().unwrap();
    *inner = Some(match inner.take() {
        Some(b) => f(a, b),
        None => a,
    });
}

fn send_from_mutex<T>(mutex: &Mutex<Option<T>>, weak: Weak<RwLock<Source<T>>>) -> CallbackResult
where
    T: Clone + Send + Sync + 'static,
{
    mutex
        .lock()
        .map_err(|_| CallbackError::Poisoned)
        .and_then(|mut inner| {
            inner
                .take()
                .map_or(Ok(()), |value| with_weak(&weak, |src| src.send(value)))
        })
}

fn send_later_from_mutex<T>(mutex: Arc<Mutex<Option<T>>>, weak: Weak<RwLock<Source<T>>>)
where
    T: Clone + Send + Sync + 'static,
{
    later(move || send_from_mutex(&mutex, weak).unwrap());
}

pub fn stream<T, F>(stream: &Stream<T>, reducer: F) -> Stream<T>
where
    T: Clone + Send + Sync + 'static,
    F: Fn(T, T) -> T + Send + Sync + 'static,
{
    let src = Arc::new(RwLock::new(Source::new()));
    let weak = Arc::downgrade(&src);
    stream.source.write().unwrap().register({
        let mutex = Arc::new(Mutex::new(None));
        move |new_value| {
            update_value(&mutex, new_value, &reducer);
            send_later_from_mutex(mutex.clone(), weak.clone());
            Ok(())
        }
    });
    Stream {
        source: src,
        keep_alive: Box::new(stream.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::update_value;
    use std::sync::Mutex;

    #[test]
    fn update_value_puts_value_in_empty_mutex() {
        let mutex = Mutex::new(None);
        update_value(&mutex, 3, |a, b| a + b);
        assert_eq!(*mutex.lock().unwrap(), Some(3));
    }

    #[test]
    fn update_value_combines_new_value_using_reducer() {
        let mutex = Mutex::new(Some(5));
        update_value(&mutex, 4, |a, b| a * b);
        assert_eq!(*mutex.lock().unwrap(), Some(20));
    }
}
