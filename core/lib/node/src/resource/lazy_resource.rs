use std::sync::{Arc, Mutex};

use thiserror::Error;
use tokio::sync::watch;

use super::Resource;
use crate::node::StopReceiver;

/// A lazy resource represent a resource that isn't available at the time when the tasks start.
/// Normally it's used to represent the resources that should be provided by one task to another one.
/// Lazy resources are aware of the node lifecycle, so attempt to resolve the resource won't hang
/// if the resource is never provided: the resolve future will fail once the stop signal is sent by the node.
#[derive(Debug)]
pub struct LazyResource<T: Resource> {
    resource: Arc<Mutex<Option<T>>>,
    resolve_receiver: watch::Receiver<bool>,
    resolve_sender: Arc<Mutex<watch::Sender<bool>>>,
    stop_receiver: StopReceiver,
}

impl<T: Resource> Clone for LazyResource<T> {
    fn clone(&self) -> Self {
        Self {
            resource: self.resource.clone(),
            resolve_receiver: self.resolve_receiver.clone(),
            resolve_sender: self.resolve_sender.clone(),
            stop_receiver: self.stop_receiver.clone(),
        }
    }
}

impl<T: Resource> LazyResource<T> {
    /// Creates a new lazy resource.
    /// Expected to be called by the node itself.
    pub(crate) fn new(stop_receiver: StopReceiver) -> Self {
        let (resolve_sender, resolve_receiver) = watch::channel(false);

        Self {
            resource: Arc::new(Mutex::new(None)),
            resolve_receiver,
            resolve_sender: Arc::new(Mutex::new(resolve_sender)),
            stop_receiver,
        }
    }

    /// Returns a future that resolves to the resource once it is provided.
    /// If the resource is never provided, the method will return an error once the node is shutting down.
    pub async fn resolve(mut self) -> Result<T, LazyResourceError> {
        tokio::select! {
            _ = self.stop_receiver.0.changed() => {
                Err(LazyResourceError::NodeShutdown)
            }
            _ = self.resolve_receiver.changed() => {
                let handle = self.resource.lock().unwrap();
                let resource = handle.as_ref().expect("Resource must be some, it was marked as provided");
                Ok(resource.clone())
            }
        }
    }

    /// Provides the resource.
    /// May be called at most once. Subsequent calls will return an error.
    pub async fn provide(&mut self, resource: T) -> Result<(), LazyResourceError> {
        let mut handle = self.resource.lock().unwrap();
        if handle.is_some() {
            return Err(LazyResourceError::ResourceAlreadyProvided);
        }

        *handle = Some(resource);

        let sender = self.resolve_sender.lock().unwrap();
        sender.send(true).unwrap();

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
