use std::{
    fmt,
    sync::{Arc, Mutex},
};

use thiserror::Error;
use tokio::sync::watch;

use super::Resource;

/// Collection of resources that can be extended during the initialization phase, and then resolved once
/// the wiring is complete.
///
/// During component initialization, resource collections can be requested by the components in order to push new
/// elements there. Once the initialization is complete, it is no longer possible to push new elements, and the
/// collection can be resolved into a vector of resources.
///
/// Collections implement `Clone`, so they can be consumed by several tasks. Every task that resolves the collection
/// is guaranteed to have the same set of resources.
///
/// The purpose of this container is to allow different tasks to register their resource in a single place for some
/// other task to consume. For example, tasks may register their healthchecks, and then healthcheck task will observe
/// all the provided healthchecks.
pub struct ResourceCollection<T> {
    /// Collection of the resources.
    resources: Arc<Mutex<Vec<T>>>,
    /// Sender indicating that the wiring is complete.
    wiring_complete_sender: Arc<watch::Sender<bool>>,
    /// Flag indicating that the collection has been resolved.
    wired: watch::Receiver<bool>,
}

impl<T: Resource> Resource for ResourceCollection<T> {
    fn on_resource_wired(&mut self) {
        self.wiring_complete_sender.send(true).ok();
    }

    fn name() -> String {
        format!("collection of {}", T::name())
    }
}

impl<T: Resource + Clone> Default for ResourceCollection<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for ResourceCollection<T> {
    fn clone(&self) -> Self {
        Self {
            resources: self.resources.clone(),
            wiring_complete_sender: self.wiring_complete_sender.clone(),
            wired: self.wired.clone(),
        }
    }
}

impl<T> fmt::Debug for ResourceCollection<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceCollection")
            .field("resources", &"{..}")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum ResourceCollectionError {
    #[error("Adding resources to the collection is not allowed after the wiring is complete")]
    AlreadyWired,
}

impl<T: Resource + Clone> ResourceCollection<T> {
    pub(crate) fn new() -> Self {
        let (wiring_complete_sender, wired) = watch::channel(false);
        Self {
            resources: Arc::default(),
            wiring_complete_sender: Arc::new(wiring_complete_sender),
            wired,
        }
    }

    /// Adds a new element to the resource collection.
    /// Returns an error if the wiring is already complete.
    pub fn push(&self, resource: T) -> Result<(), ResourceCollectionError> {
        // This check is sufficient, since no task is guaranteed to be running when the value changes.
        if *self.wired.borrow() {
            return Err(ResourceCollectionError::AlreadyWired);
        }

        let mut handle = self.resources.lock().unwrap();
        handle.push(resource);
        tracing::info!(
            "A new item has been added to the resource collection {}",
            Self::name()
        );
        Ok(())
    }

    /// Waits until the wiring is complete, and resolves the collection into a vector of resources.
    pub async fn resolve(mut self) -> Vec<T> {
        // Guaranteed not to hang on server shutdown, since the node will invoke the `on_wiring_complete` before any task
        // is actually spawned (per framework rules). For most cases, this check will resolve immediately, unless
        // some tasks would spawn something from the `IntoZkSyncTask` impl.
        self.wired.changed().await.expect("Sender can't be dropped");

        tracing::info!("Resource collection {} has been resolved", Self::name());

        let handle = self.resources.lock().unwrap();
        (*handle).clone()
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use futures::FutureExt;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestResource(Arc<u8>);

    impl Resource for TestResource {
        fn name() -> String {
            "test_resource".into()
        }
    }

    #[test]
    fn test_push() {
        let collection = ResourceCollection::<TestResource>::new();
        let resource1 = TestResource(Arc::new(1));
        collection.clone().push(resource1.clone()).unwrap();

        let resource2 = TestResource(Arc::new(2));
        collection.clone().push(resource2.clone()).unwrap();

        assert_eq!(
            *collection.resources.lock().unwrap(),
            vec![resource1, resource2]
        );
    }

    #[test]
    fn test_already_wired() {
        let mut collection = ResourceCollection::<TestResource>::new();
        let resource = TestResource(Arc::new(1));

        let rc_clone = collection.clone();

        collection.on_resource_wired();

        assert_matches!(
            rc_clone.push(resource),
            Err(ResourceCollectionError::AlreadyWired)
        );
    }

    #[test]
    fn test_resolve() {
        let mut collection = ResourceCollection::<TestResource>::new();
        let result = collection.clone().resolve().now_or_never();

        assert!(result.is_none());

        collection.on_resource_wired();

        let resolved = collection.resolve().now_or_never();
        assert_eq!(resolved.unwrap(), vec![]);
    }
}
