use std::{any::Any, cell::RefCell, collections::HashMap, fmt};

use futures::{future::BoxFuture, FutureExt};
use tokio::{runtime::Runtime, sync::watch};
use zksync_health_check::CheckHealth;

use crate::{
    resource::{stop_receiver::StopReceiverResource, Resource, ResourceProvider},
    task::{TaskInitError, ZkSyncTask},
};

/// An interface to the node's resources provided to the tasks during initialization.
/// Provides the ability to fetch required resources, and also gives access to the Tokio runtime used by the node.
#[derive(Debug)]
pub struct NodeContext<'a> {
    node: &'a ZkSyncNode,
}

impl<'a> NodeContext<'a> {
    fn new(node: &'a ZkSyncNode) -> Self {
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
    pub fn get_resource<T: Resource>(&self, name: impl AsRef<str>) -> Option<T> {
        let downcast_clone = |resource: &Box<dyn Any>| {
            resource
                .downcast_ref::<T>()
                .unwrap_or_else(|| {
                    panic!(
                        "Resource {} is not of type {}",
                        name.as_ref(),
                        std::any::type_name::<T>()
                    )
                })
                .clone()
        };

        let name = name.as_ref();
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

type TaskConstructor =
    Box<dyn FnOnce(&NodeContext<'_>) -> Result<Box<dyn ZkSyncTask>, TaskInitError>>;
type HealthCheckTaskConstructor = Box<
    dyn FnOnce(
        &NodeContext<'_>,
        Vec<Box<dyn CheckHealth>>,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError>,
>;

/// "Manager" class of the node. Collects all the resources and tasks,
/// then runs tasks until completion.
pub struct ZkSyncNode {
    resource_provider: Box<dyn ResourceProvider>,
    /// Anything that may be needed by 1 or more tasks to be created.
    /// Think `ConnectionPool`, `TxSender`, `SealManager`, `EthGateway`, `GasAdjuster`.
    /// There is a note on `dyn Any` below the snippet.
    resources: RefCell<HashMap<String, Box<dyn Any>>>,
    /// List of task constructors.
    task_constructors: Vec<(String, TaskConstructor)>,
    /// Optional constructor for the healthcheck task.
    healthcheck_task_constructor: Option<HealthCheckTaskConstructor>,

    stop_sender: watch::Sender<bool>,
    /// Tokio runtime used to spawn tasks.
    /// During the node initialization the implicit tokio context is not available, so tasks
    /// are expected to use the handle provided by [`NodeContext`].
    runtime: Runtime,
}

impl fmt::Debug for ZkSyncNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: Provide better impl
        f.debug_struct("ZkSyncNode").finish_non_exhaustive()
    }
}

impl ZkSyncNode {
    pub fn new<R: ResourceProvider>(resource_provider: R) -> anyhow::Result<Self> {
        if tokio::runtime::Handle::try_current().is_ok() {
            anyhow::bail!(
                "Detected a Tokio Runtime. ZkSyncNode manages its own runtime and does not support nested runtimes"
            );
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let (stop_sender, stop_receiver) = watch::channel(false);
        let mut self_ = Self {
            resource_provider: Box::new(resource_provider),
            resources: RefCell::default(),
            task_constructors: Vec::new(),
            healthcheck_task_constructor: None,
            stop_sender,
            runtime,
        };
        self_.add_resource(
            StopReceiverResource::RESOURCE_NAME,
            StopReceiverResource::new(stop_receiver),
        );

        Ok(self_)
    }

    /// Stores a resource in the internal cache.
    fn add_resource<T: Resource>(&mut self, name: impl AsRef<str>, resource: T) {
        self.resources
            .borrow_mut()
            .insert(name.as_ref().into(), Box::new(resource));
    }

    /// Adds a task to the node.
    ///
    /// The task is not created at this point, instead, the constructor is stored in the node
    /// and will be invoked during [`ZkSyncNode::run`] method. Any error returned by the constructor
    /// will prevent the node from starting and will be propagated by the [`ZkSyncNode::run`] method.
    pub fn add_task<
        F: FnOnce(&NodeContext<'_>) -> Result<Box<dyn ZkSyncTask>, TaskInitError> + 'static,
    >(
        &mut self,
        name: impl AsRef<str>,
        task_constructor: F,
    ) -> &mut Self {
        self.task_constructors
            .push((name.as_ref().into(), Box::new(task_constructor)));
        self
    }

    /// Adds a healtcheck task to the node.
    ///
    /// Healtcheck task is treated as any other task, with the following differences:
    /// - It is given the list of healthchecks of all the other tasks.
    /// - Its own healtcheck is ignored (the task is expected to handle its own checks itself).
    /// - There may be only one healthcheck task per node.
    ///
    /// # Panics
    ///
    /// Panics if the healthcheck task is already set.
    pub fn with_healthcheck<
        F: FnOnce(
                &NodeContext<'_>,
                Vec<Box<dyn CheckHealth>>,
            ) -> Result<Box<dyn ZkSyncTask>, TaskInitError>
            + 'static,
    >(
        &mut self,
        healthcheck_task_constructor: F,
    ) -> &mut Self {
        assert!(
            self.healthcheck_task_constructor.is_none(),
            "Healthcheck task is already set"
        );
        self.healthcheck_task_constructor = Some(Box::new(healthcheck_task_constructor));
        self
    }

    /// Runs the system.
    pub fn run(mut self) -> anyhow::Result<()> {
        // Initialize tasks.
        let task_constructors = std::mem::take(&mut self.task_constructors);

        let mut tasks = Vec::new();
        let mut healthchecks = Vec::new();

        let mut errors: Vec<(String, TaskInitError)> = Vec::new();

        for (name, task_constructor) in task_constructors {
            let mut task = match task_constructor(&NodeContext::new(&self)) {
                Ok(task) => task,
                Err(err) => {
                    // We don't want to bail on the first error, since it'll provide worse DevEx:
                    // People likely want to fix as much problems as they can in one go, rather than have
                    // to fix them one by one.
                    errors.push((name, err));
                    continue;
                }
            };
            let healthcheck = task.healtcheck();
            let after_node_shutdown = task.after_node_shutdown();
            let task_future = Box::pin(task.run());
            let task_repr = TaskRepr {
                name,
                task: Some(task_future),
                after_node_shutdown,
            };
            tasks.push(task_repr);
            if let Some(healthcheck) = healthcheck {
                healthchecks.push(healthcheck);
            }
        }

        // Report all the errors we've met during the init.
        if !errors.is_empty() {
            for (task, error) in errors {
                tracing::error!("Task {task} can't be initialized: {error}");
            }
            anyhow::bail!("One or more task weren't able to start");
        }

        if tasks.is_empty() {
            anyhow::bail!("No tasks to run");
        }

        // Initialize the healthcheck task, if any.
        if let Some(healthcheck_constructor) = self.healthcheck_task_constructor.take() {
            let healthcheck_task = healthcheck_constructor(&NodeContext::new(&self), healthchecks)
                .map_err(|err| {
                    tracing::error!("Healthcheck task can't be initialized: {err}");
                    anyhow::format_err!("Healtcheck task can't be initialized: {err}")
                })?;
            // We don't call `healthcheck_task.healtcheck()` here, as we expect it to know its own health.
            let after_node_shutdown = healthcheck_task.after_node_shutdown();
            let task_future = Box::pin(healthcheck_task.run());
            let task_repr = TaskRepr {
                name: "healthcheck".into(),
                task: Some(task_future),
                after_node_shutdown,
            };
            tasks.push(task_repr);
        }

        // Prepare tasks for running.
        let rt_handle = self.runtime.handle().clone();
        let join_handles: Vec<_> = tasks
            .iter_mut()
            .map(|task| {
                let task = task.task.take().expect(
                    "Tasks are created by the node and must be Some prior to calling this method",
                );
                rt_handle.spawn(task).fuse()
            })
            .collect();

        // Run the tasks until one of them exits.
        // TODO: wrap every task into a timeout to prevent hanging.
        let (resolved, idx, remaining) = self
            .runtime
            .block_on(futures::future::select_all(join_handles));
        let task_name = tasks[idx].name.clone();
        let failure = match resolved {
            Ok(Ok(())) => {
                tracing::info!("Task {task_name} completed");
                false
            }
            Ok(Err(err)) => {
                tracing::error!("Task {task_name} exited with an error: {err}");
                true
            }
            Err(_) => {
                tracing::error!("Task {task_name} panicked");
                true
            }
        };

        // Send stop signal to remaining tasks and wait for them to finish.
        // Given that we are shutting down, we do not really care about returned values.
        self.stop_sender.send(true).ok();
        self.runtime.block_on(futures::future::join_all(remaining));

        // Call after_node_shutdown hooks.
        let local_set = tokio::task::LocalSet::new();
        let join_handles = tasks.iter_mut().filter_map(|task| {
            task.after_node_shutdown
                .take()
                .map(|task| local_set.spawn_local(task))
        });
        local_set.block_on(&self.runtime, futures::future::join_all(join_handles));

        if failure {
            anyhow::bail!("Task {task_name} failed");
        } else {
            Ok(())
        }
    }
}

struct TaskRepr {
    name: String,
    task: Option<BoxFuture<'static, anyhow::Result<()>>>,
    after_node_shutdown: Option<BoxFuture<'static, ()>>,
}

impl fmt::Debug for TaskRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskRepr")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
