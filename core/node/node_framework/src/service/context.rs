use std::any::type_name;

use super::shutdown_hook::ShutdownHook;
use crate::{
    resource::{Resource, ResourceId, StoredResource},
    service::{named_future::NamedFuture, ZkStackService},
    task::Task,
    wiring_layer::WiringError,
};

/// An interface to the service provided to the tasks during initialization.
/// This the main point of interaction between with the service.
///
/// The context provides access to the runtime, resources, and allows adding new tasks.
#[derive(Debug)]
pub struct ServiceContext<'a> {
    layer: &'a str,
    service: &'a mut ZkStackService,
}

impl<'a> ServiceContext<'a> {
    /// Instantiates a new context.
    /// The context keeps information about the layer that created it for reporting purposes.
    pub(super) fn new(layer: &'a str, service: &'a mut ZkStackService) -> Self {
        Self { layer, service }
    }

    /// Provides access to the runtime used by the service.
    ///
    /// Can be used to spawn additional tasks within the same runtime.
    /// If some task stores the handle to spawn additional tasks, it is expected to do all the required
    /// cleanup.
    ///
    /// In most cases, however, it is recommended to use [`add_task`](ServiceContext::add_task) or its alternative
    /// instead.
    ///
    /// ## Note
    ///
    /// While `tokio::spawn` and `tokio::spawn_blocking` will work as well, using the runtime handle
    /// from the context is still a recommended way to get access to runtime, as it tracks the access
    /// to the runtimes by layers.
    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        tracing::info!(
            "Layer {} has requested access to the Tokio runtime",
            self.layer
        );
        self.service.runtime.handle()
    }

    /// Adds a task to the service.
    ///
    /// Added tasks will be launched after the wiring process will be finished and all the preconditions
    /// are met.
    pub fn add_task<T: Task>(&mut self, task: T) -> &mut Self {
        tracing::info!("Layer {} has added a new task: {}", self.layer, task.id());
        self.service.runnables.tasks.push(Box::new(task));
        self
    }

    /// Adds a future to be invoked after node shutdown.
    /// May be used to perform cleanup tasks.
    ///
    /// The future is guaranteed to only be polled after all the node tasks are stopped or timed out.
    /// All the futures will be awaited sequentially.
    pub fn add_shutdown_hook(&mut self, hook: ShutdownHook) -> &mut Self {
        tracing::info!(
            "Layer {} has added a new shutdown hook: {}",
            self.layer,
            hook.id
        );
        self.service
            .runnables
            .shutdown_hooks
            .push(NamedFuture::new(hook.future, hook.id));
        self
    }

    /// Attempts to retrieve the resource of the specified type.
    ///
    /// ## Panics
    ///
    /// Panics if the resource with the specified [`ResourceId`] exists, but is not of the requested type.
    pub fn get_resource<T: Resource + Clone>(&mut self) -> Result<T, WiringError> {
        // Implementation details:
        // Internally the resources are stored as [`std::any::Any`], and this method does the downcasting
        // on behalf of the caller.
        #[allow(clippy::borrowed_box)]
        let downcast_clone = |resource: &Box<dyn StoredResource>| {
            resource
                .downcast_ref::<T>()
                .unwrap_or_else(|| {
                    panic!(
                        "Resource {} is not of type {}",
                        T::name(),
                        std::any::type_name::<T>()
                    )
                })
                .clone()
        };

        // Check whether the resource is already available.
        if let Some(resource) = self.service.resources.get(&ResourceId::of::<T>()) {
            tracing::info!(
                "Layer {} has requested resource {} of type {}",
                self.layer,
                T::name(),
                type_name::<T>()
            );
            return Ok(downcast_clone(resource));
        }

        tracing::info!(
            "Layer {} has requested resource {} of type {}, but it is not available",
            self.layer,
            T::name(),
            type_name::<T>()
        );

        // No such resource.
        // The requester is allowed to decide whether this is an error or not.
        Err(WiringError::ResourceLacking {
            name: T::name(),
            id: ResourceId::of::<T>(),
        })
    }

    /// Attempts to retrieve the resource of the specified type.
    /// If the resource is not available, it is created using the provided closure.
    pub fn get_resource_or_insert_with<T: Resource + Clone, F: FnOnce() -> T>(
        &mut self,
        f: F,
    ) -> T {
        if let Ok(resource) = self.get_resource::<T>() {
            return resource;
        }

        // No such resource, insert a new one.
        let resource = f();
        self.service
            .resources
            .insert(ResourceId::of::<T>(), Box::new(resource.clone()));
        tracing::info!(
            "Layer {} has created a new resource {}",
            self.layer,
            T::name()
        );
        resource
    }

    /// Attempts to retrieve the resource of the specified type.
    /// If the resource is not available, it is created using `T::default()`.
    pub fn get_resource_or_default<T: Resource + Clone + Default>(&mut self) -> T {
        self.get_resource_or_insert_with(T::default)
    }

    /// Adds a resource to the service.
    ///
    /// If the resource with the same type is already provided, the method will return an error.
    pub fn insert_resource<T: Resource>(&mut self, resource: T) -> Result<(), WiringError> {
        let id = ResourceId::of::<T>();
        if self.service.resources.contains_key(&id) {
            tracing::info!(
                "Layer {} has attempted to provide resource {} of type {}, but it is already available",
                self.layer,
                T::name(),
                type_name::<T>()
            );
            return Err(WiringError::ResourceAlreadyProvided {
                id: ResourceId::of::<T>(),
                name: T::name(),
            });
        }
        self.service.resources.insert(id, Box::new(resource));
        tracing::info!(
            "Layer {} has provided a new resource {}",
            self.layer,
            T::name()
        );
        Ok(())
    }
}
