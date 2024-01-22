use crate::{
    node::ZkSyncNode,
    resource::{Resource, StoredResource},
    task::IntoZkSyncTask,
};

/// An interface to the node's resources provided to the tasks during initialization.
/// Provides the ability to fetch required resources, and also gives access to the Tokio runtime handle used by the node.
#[derive(Debug)]
pub struct NodeContext<'a> {
    node: &'a mut ZkSyncNode,
}

impl<'a> NodeContext<'a> {
    pub(super) fn new(node: &'a mut ZkSyncNode) -> Self {
        Self { node }
    }

    /// Provides access to the runtime used by the node.
    /// Can be used to spawn additional tasks within the same runtime.
    /// If some tasks stores the handle to spawn additional tasks, it is expected to do all the required
    /// cleanup.
    ///
    /// In most cases, however, it is recommended to use [`add_task`] method instead.
    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        self.node.runtime.handle()
    }

    /// Adds an additional task to the node.
    /// This may be used if some task or its resource requires an additional routine for maintenance.
    pub fn add_task<T: IntoZkSyncTask>(&mut self, builder: T) -> &mut Self {
        self.node.add_task(builder);
        self
    }

    /// Attempts to retrieve the resource with the specified name.
    /// Internally the resources are stored as [`std::any::Any`], and this method does the downcasting
    /// on behalf of the caller.
    ///
    /// ## Panics
    ///
    /// Panics if the resource with the specified name exists, but is not of the requested type.
    pub async fn get_resource<T: Resource + Clone>(&mut self) -> Option<T> {
        #[allow(clippy::borrowed_box)]
        let downcast_clone = |resource: &Box<dyn StoredResource>| {
            resource
                .downcast_ref::<T>()
                .unwrap_or_else(|| {
                    panic!(
                        "Resource {} is not of type {}",
                        T::resource_id(),
                        std::any::type_name::<T>()
                    )
                })
                .clone()
        };

        let name = T::resource_id();
        // Check whether the resource is already available.
        if let Some(resource) = self.node.resources.get(&name) {
            return Some(downcast_clone(resource));
        }

        // Try to fetch the resource from the provider.
        if let Some(resource) = self.node.resource_provider.get_resource(&name).await {
            // First, ensure the type matches.
            let downcasted = downcast_clone(&resource);
            // Then, add it to the local resources.
            self.node.resources.insert(name, resource);
            return Some(downcasted);
        }

        // No such resource.
        // The requester is allowed to decide whether this is an error or not.
        None
    }

    /// Attempts to retrieve the resource with the specified name.
    /// If the resource is not available, it is created using the provided closure.
    pub async fn get_resource_or_insert_with<T: Resource + Clone, F: FnOnce() -> T>(
        &mut self,
        f: F,
    ) -> T {
        if let Some(resource) = self.get_resource::<T>().await {
            return resource;
        }

        // No such resource, insert a new one.
        let resource = f();
        self.node
            .resources
            .insert(T::resource_id(), Box::new(resource.clone()));
        resource
    }

    /// Attempts to retrieve the resource with the specified name.
    /// If the resource is not available, it is created using `T::default()`.
    pub async fn get_resource_or_default<T: Resource + Clone + Default>(&mut self) -> T {
        self.get_resource_or_insert_with(T::default).await
    }
}
