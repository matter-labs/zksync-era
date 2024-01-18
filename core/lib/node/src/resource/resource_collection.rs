use std::{
    fmt,
    sync::{atomic::AtomicBool, Arc, RwLock},
};

use thiserror::Error;
use tokio::sync::watch;

/// Collection of resources that can be extended during the initialization phase,
/// and then resolved once the wiring is complete.
///
/// During component initialization, resource collections can be requested by the components in order
/// to push new elements there. Once the initialization is complete, it is no longer possible to push
/// new elements, and the collection can be resolved into a vector of resources.
///
/// Collections are meant to be consumed by a single task: if multiple tasks will try to resolve the
/// same collection, the first one to do so will succeed, and the rest will receive an error. If you need a
/// collection that will be used by multiple tasks, consider creating a dedicated resource which would wrap
/// some collection, like `Arc<RwLock<T>>`.
///
/// Note that the collection type doesn't have to implement the [Resource] trait, since it is the collection
/// itself that is a resource, not the elements it contains.
///
/// The purpose of this container is to allow different tasks to register their resource in a single place for some
/// other task to consume. For example, tasks may register their healthchecks, and then healthcheck task will observe
/// all the provided healthchecks.
pub struct ResourceCollection<T: 'static> {
    /// Collection of the resources.
    resources: Arc<RwLock<Vec<T>>>,
    /// Whether someone took the value from this collection.
    resolved: Arc<AtomicBool>,
    /// Whether the wiring process has been completed.
    wired: watch::Receiver<bool>,
}

impl<T> Clone for ResourceCollection<T> {
    fn clone(&self) -> Self {
        Self {
            resources: self.resources.clone(),
            resolved: self.resolved.clone(),
            wired: self.wired.clone(),
        }
    }
}

impl<T> fmt::Debug for ResourceCollection<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceCollection")
            .field("resources", &"{..}")
            .field("resolved", &self.resolved)
            .field("wired", &self.wired)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum ResourceCollectionError {
    #[error("Adding resources to the collection is not allowed after the wiring is complete")]
    AlreadyWired,
    #[error("Resource collection is already resolved by the component {0}")]
    AlreadyResolved(&'static str),
}

impl<T: 'static> ResourceCollection<T> {
    pub(crate) fn new(wired: watch::Receiver<bool>) -> Self {
        Self {
            resources: Default::default(),
            resolved: Default::default(),
            wired,
        }
    }

    pub fn push(&self, resource: T) -> Result<(), ResourceCollectionError> {
        // This check is sufficient, since no task is guaranteed to be running when the value changes.
        if *self.wired.borrow() {
            return Err(ResourceCollectionError::AlreadyWired);
        }

        let mut handle = self.resources.write().unwrap();
        handle.push(resource);
        Ok(())
    }

    pub async fn resolve(mut self) -> Result<Vec<T>, ResourceCollectionError> {
        // Guaranteed not to hang on server shutdown, since the node will change the value before any task is
        // actually spawned (per framework rules). For most cases, this check will resolve immediately, unless
        // some tasks would spawn something from the `IntoZkSyncTask` impl.
        self.wired
            .changed()
            .await
            .map_err(|_| ResourceCollectionError::AlreadyResolved(std::any::type_name::<T>()))?;

        let mut handle = self.resources.write().unwrap();
        if self.resolved.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ResourceCollectionError::AlreadyResolved(
                std::any::type_name::<T>(),
            ));
        }
        let resources = std::mem::take(&mut *handle);
        self.resolved
            .store(true, std::sync::atomic::Ordering::Relaxed);
        drop(handle);

        Ok(resources)
    }
}
