use std::{
    fmt,
    sync::{Arc, Mutex},
};

use thiserror::Error;
use tokio::sync::watch;

use super::{Resource, ResourceId};

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
    fn resource_id() -> ResourceId {
        ResourceId::new("collection") + T::resource_id()
    }

    fn on_resource_wired(&mut self) {
        self.wiring_complete_sender.send(true).ok();
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
        Ok(())
    }

    /// Waits until the wiring is complete, and resolves the collection into a vector of resources.
    pub async fn resolve(mut self) -> Vec<T> {
        // Guaranteed not to hang on server shutdown, since the node will invoke the `on_wiring_complete` before any task
        // is actually spawned (per framework rules). For most cases, this check will resolve immediately, unless
        // some tasks would spawn something from the `IntoZkSyncTask` impl.
        self.wired.changed().await.expect("Sender can't be dropped");

        let handle = self.resources.lock().unwrap();
        (*handle).clone()
    }
}
