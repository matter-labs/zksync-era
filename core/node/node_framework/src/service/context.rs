use std::any::type_name;

use crate::{
    precondition::Precondition,
    resource::{Resource, ResourceId, StoredResource},
    service::ZkStackService,
    task::{OneshotTask, Task, UnconstrainedOneshotTask, UnconstrainedTask},
    wiring_layer::WiringError,
};

/// An interface to the service's resources provided to the tasks during initialization.
/// Provides the ability to fetch required resources, and also gives access to the Tokio runtime handle.
#[derive(Debug)]
pub struct ServiceContext<'a> {
    layer: &'a str,
    service: &'a mut ZkStackService,
}

impl<'a> ServiceContext<'a> {
    pub(super) fn new(layer: &'a str, service: &'a mut ZkStackService) -> Self {
        Self { layer, service }
    }

    /// Provides access to the runtime used by the service.
    /// Can be used to spawn additional tasks within the same runtime.
    /// If some tasks stores the handle to spawn additional tasks, it is expected to do all the required
    /// cleanup.
    ///
    /// In most cases, however, it is recommended to use [`add_task`] method instead.
    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        tracing::info!(
            "Layer {} has requested access to the Tokio runtime",
            self.layer
        );
        self.service.runtime.handle()
    }

    /// Adds a task to the service.
    /// Added tasks will be launched after the wiring process will be finished and all the preconditions
    /// are met.
    pub fn add_task(&mut self, task: Box<dyn Task>) -> &mut Self {
        tracing::info!("Layer {} has added a new task: {}", self.layer, task.name());
        self.service.runnables.tasks.push(task);
        self
    }

    /// Adds an unconstrained task to the service.
    /// Unconstrained tasks will be launched immediately after the wiring process is finished.
    pub fn add_unconstrained_task(&mut self, task: Box<dyn UnconstrainedTask>) -> &mut Self {
        tracing::info!(
            "Layer {} has added a new unconstrained task: {}",
            self.layer,
            task.name()
        );
        self.service.runnables.unconstrained_tasks.push(task);
        self
    }

    /// Adds a precondition to the service.
    pub fn add_precondition(&mut self, precondition: Box<dyn Precondition>) -> &mut Self {
        tracing::info!(
            "Layer {} has added a new precondition: {}",
            self.layer,
            precondition.name()
        );
        self.service.runnables.preconditions.push(precondition);
        self
    }

    /// Adds an oneshot task to the service.
    pub fn add_oneshot_task(&mut self, task: Box<dyn OneshotTask>) -> &mut Self {
        tracing::info!(
            "Layer {} has added a new oneshot task: {}",
            self.layer,
            task.name()
        );
        self.service.runnables.oneshot_tasks.push(task);
        self
    }

    /// Adds an unconstrained oneshot task to the service.
    pub fn add_unconstrained_oneshot_task(
        &mut self,
        task: Box<dyn UnconstrainedOneshotTask>,
    ) -> &mut Self {
        tracing::info!(
            "Layer {} has added a new unconstrained oneshot task: {}",
            self.layer,
            task.name()
        );
        self.service
            .runnables
            .unconstrained_oneshot_tasks
            .push(task);
        self
    }

    /// Attempts to retrieve the resource with the specified name.
    /// Internally the resources are stored as [`std::any::Any`], and this method does the downcasting
    /// on behalf of the caller.
    ///
    /// ## Panics
    ///
    /// Panics if the resource with the specified name exists, but is not of the requested type.
    pub async fn get_resource<T: Resource + Clone>(&mut self) -> Result<T, WiringError> {
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

    /// Attempts to retrieve the resource with the specified name.
    /// If the resource is not available, it is created using the provided closure.
    pub async fn get_resource_or_insert_with<T: Resource + Clone, F: FnOnce() -> T>(
        &mut self,
        f: F,
    ) -> T {
        if let Ok(resource) = self.get_resource::<T>().await {
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

    /// Attempts to retrieve the resource with the specified name.
    /// If the resource is not available, it is created using `T::default()`.
    pub async fn get_resource_or_default<T: Resource + Clone + Default>(&mut self) -> T {
        self.get_resource_or_insert_with(T::default).await
    }

    /// Adds a resource to the service.
    /// If the resource with the same name is already provided, the method will return an error.
    pub fn insert_resource<T: Resource>(&mut self, resource: T) -> Result<(), WiringError> {
        let id = ResourceId::of::<T>();
        if self.service.resources.contains_key(&id) {
            tracing::warn!(
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
