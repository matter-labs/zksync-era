use std::sync::Arc;

use thiserror::Error;
use tokio::sync::watch;

use super::{Resource, ResourceId};
use crate::node::StopReceiver;

/// A lazy resource represents a resource that isn't available at the time when the tasks start.
///
/// Normally it's used to represent the resources that should be provided by one task to another one.
/// Lazy resources are aware of the node lifecycle, so attempt to resolve the resource won't hang
/// if the resource is never provided: the resolve future will fail once the stop signal is sent by the node.
#[derive(Debug)]
pub struct LazyResource<T: Resource> {
    resolve_sender: Arc<watch::Sender<Option<T>>>,
    stop_receiver: StopReceiver,
}

impl<T: Resource> Resource for LazyResource<T> {
    fn resource_id() -> ResourceId {
        ResourceId::new("lazy") + T::resource_id()
    }
}

impl<T: Resource> Clone for LazyResource<T> {
    fn clone(&self) -> Self {
        Self {
            resolve_sender: self.resolve_sender.clone(),
            stop_receiver: self.stop_receiver.clone(),
        }
    }
}

impl<T: Resource + Clone> LazyResource<T> {
    /// Creates a new lazy resource.
    /// Provided stop receiver will be used to prevent resolving from hanging if the resource is never provided.
    pub fn new(stop_receiver: StopReceiver) -> Self {
        let (resolve_sender, _resolve_receiver) = watch::channel(None);

        Self {
            resolve_sender: Arc::new(resolve_sender),
            stop_receiver,
        }
    }

    /// Returns a future that resolves to the resource once it is provided.
    /// If the resource is never provided, the method will return an error once the node is shutting down.
    pub async fn resolve(mut self) -> Result<T, LazyResourceError> {
        let mut resolve_receiver = self.resolve_sender.subscribe();
        if let Some(resource) = resolve_receiver.borrow().as_ref() {
            return Ok(resource.clone());
        }

        tokio::select! {
            _ = self.stop_receiver.0.changed() => {
                Err(LazyResourceError::NodeShutdown)
            }
            _ = resolve_receiver.changed() => {
                // ^ we can ignore the error on `changed`, since we hold a strong reference to the sender.
                let resource = resolve_receiver.borrow().as_ref().expect("Can only change if provided").clone();
                Ok(resource)
            }
        }
    }

    /// Provides the resource.
    /// May be called at most once. Subsequent calls will return an error.
    pub async fn provide(&mut self, resource: T) -> Result<(), LazyResourceError> {
        let sent = self.resolve_sender.send_if_modified(|current| {
            if current.is_some() {
                return false;
            }
            *current = Some(resource.clone());
            true
        });

        if !sent {
            return Err(LazyResourceError::ResourceAlreadyProvided);
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum LazyResourceError {
    #[error("Node is shutting down")]
    NodeShutdown,
    #[error("Resource is already provided")]
    ResourceAlreadyProvided,
}
