use std::sync::{Arc, Mutex};

/// Wrapper for resources that only support one consumer.
///
/// Normally, all the resources are expected to be shareable between several tasks,
/// but there are some cases where a resource should only be consumed by a single task.
#[derive(Debug)]
pub struct Unique<T: 'static + Send> {
    inner: Arc<Mutex<Option<T>>>,
}

impl<T> Clone for Unique<T>
where
    T: 'static + Send,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static + Send> Unique<T> {
    /// Creates a new unique resource.
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(inner))),
        }
    }

    /// Takes the resource from the container.
    pub fn take(&self) -> Option<T> {
        self.inner.lock().unwrap().take()
    }
}
