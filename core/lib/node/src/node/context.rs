use std::any::Any;

use crate::{node::ZkSyncNode, resource::Resource};

/// An interface to the node's resources provided to the tasks during initialization.
/// Provides the ability to fetch required resources, and also gives access to the Tokio runtime used by the node.
#[derive(Debug)]
pub struct NodeContext<'a> {
    node: &'a ZkSyncNode,
}

impl<'a> NodeContext<'a> {
    pub(super) fn new(node: &'a ZkSyncNode) -> Self {
        Self { node }
    }

    /// Provides access to the runtime used by the node.
    /// Can be used to execute non-blocking code in the task constructors, or to spawn additional tasks within
    /// the same runtime.
    /// If some tasks stores the handle to spawn additional tasks, it is considered responsible for all the
    /// required cleanup.
    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        self.node.runtime.handle()
    }

    /// Attempts to retrieve the resource with the specified name.
    /// Internally the resources are stored as [`std::any::Any`], and this method does the downcasting
    /// on behalf of the caller.
    ///
    /// ## Panics
    ///
    /// Panics if the resource with the specified name exists, but is not of the requested type.
    pub fn get_resource<T: Resource>(&self) -> Option<T> {
        let downcast_clone = |resource: &Box<dyn Any>| {
            resource
                .downcast_ref::<T>()
                .unwrap_or_else(|| {
                    panic!(
                        "Resource {} is not of type {}",
                        T::RESOURCE_NAME,
                        std::any::type_name::<T>()
                    )
                })
                .clone()
        };

        let name = T::RESOURCE_NAME;
        // Check whether the resource is already available.
        if let Some(resource) = self.node.resources.borrow().get(name) {
            return Some(downcast_clone(resource));
        }

        // Try to fetch the resource from the provider.
        if let Some(resource) = self.node.resource_provider.get_resource(name) {
            // First, ensure the type matches.
            let downcasted = downcast_clone(&resource);
            // Then, add it to the local resources.
            self.node
                .resources
                .borrow_mut()
                .insert(name.into(), resource);
            return Some(downcasted);
        }

        // No such resource.
        // The requester is allowed to decide whether this is an error or not.
        None
    }
}
