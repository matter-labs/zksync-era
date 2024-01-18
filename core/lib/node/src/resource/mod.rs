use std::{
    any::Any,
    fmt,
    sync::{atomic::AtomicBool, Arc, RwLock},
};

use thiserror::Error;
use tokio::sync::watch;

/// A marker trait for anything that can be stored (and retrieved) as a resource.
/// Requires `Clone` since the same resource may be requested by several tasks.
pub trait Resource: 'static + Clone + std::any::Any {
    /// Unique identifier of the resource.
    /// Used to fetch the resource from the provider.
    ///
    /// It is recommended to name resources in form of `<scope>/<name>`, where `<scope>` is the name of the task
    /// that will use this resource, or 'common' in case it is used by several tasks, and `<name>` is the name
    /// of the resource itself.
    const RESOURCE_NAME: &'static str;
}

/// An entity that knows how to initialize resources.
///
/// It exists to simplify the initialization process, as both tasks and *resources* can depend on other resources,
/// and by having an entity that can initialize the resource on demand we can avoid the need to provide resources
/// in any particular order.
///
/// Node will only call `get_resource` method once per resource, and will cache the result. This guarantees that
/// all the resource consumers will interact with the same resource instance, which may be important for getting
/// the consistent state (e.g. to make sure that L1 gas price is the same for all the tasks).
pub trait ResourceProvider: 'static + Send + Sync + fmt::Debug {
    /// Returns a resource with the given name.
    ///
    /// In case it isn't possible to obtain the resource (for example, if some error occurred during initialization),
    /// the provider is free to either return `None` (if it assumes that the node can continue without this resource),
    /// or to panic.
    // Note: we have to use `Box<dyn Any>` here, since we can't use `Box<dyn Resource>` due to it not being object-safe.
    fn get_resource(&self, name: &str) -> Option<Box<dyn Any>>;
}

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
/// The purpose of this container is to allow different tasks to register their resource in a single place for som
/// other task to consume. For example, tasks may register their healtchecks, and then healtcheck task will observe
/// all the provided healtchecks.
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
