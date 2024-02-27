use std::sync::{Arc, Mutex};

/// Wrapper for resources that are only intended for single consumption.
///
/// Typically, resources are designed for sharing among multiple tasks. However, there are scenarios where
/// a resource should only be consumed by a single task.
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

    /// Retrieves and removes the resource from the container.
    pub fn take(&self) -> Option<T> {
        let result = self.inner.lock().unwrap().take();

        if result.is_some() {
            tracing::info!(
                "Resource {} has been retrieved and removed",
                std::any::type_name::<Unique<T>>()
            );
        }

        result
    }
}
