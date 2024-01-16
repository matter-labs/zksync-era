use std::{any::Any, cell::RefCell, collections::HashMap, fmt};

use futures::{future::BoxFuture, FutureExt};
use tokio::{runtime::Runtime, sync::watch};
use zksync_health_check::CheckHealth;

pub use self::{context::NodeContext, stop_receiver::StopReceiver};
use crate::{
    resource::ResourceProvider,
    task::{TaskInitError, ZkSyncTask},
};

mod context;
mod stop_receiver;

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
///
/// Initialization flow:
/// - Node instance is created with access to the resource provider.
/// - Task constructors are added to the node. At this step, tasks are not created yet.
/// - Optionally, a healthcheck task constructor is also added.
/// - Once the `run` method is invoked, node
///   - attempts to create every task. If there are no tasks, or at least task
///     constructor fails, the node will return an error.
///   - initializes the healthcheck task if it's provided.
///   - waits for any of the tasks to finish.
///   - sends stop signal to all the tasks.
///   - waits for the remaining tasks to finish.
///   - calls `after_node_shutdown` hook for every task that has provided it.
///   - returns the result of the task that has finished.
pub struct ZkSyncNode {
    /// Primary source of resources for tasks.
    resource_provider: Box<dyn ResourceProvider>,
    /// Cache of resources that have been requested at least by one task.
    resources: RefCell<HashMap<String, Box<dyn Any>>>,
    /// List of task constructors.
    task_constructors: Vec<(String, TaskConstructor)>,
    /// Optional constructor for the healthcheck task.
    healthcheck_task_constructor: Option<HealthCheckTaskConstructor>,

    /// Sender used to stop the tasks.
    stop_sender: watch::Sender<bool>,
    /// Stop receiver to be shared with tasks.
    stop_receiver: StopReceiver,
    /// Tokio runtime used to spawn tasks.
    /// During the node initialization the implicit tokio context is not available, so tasks
    /// are expected to use the handle provided by [`NodeContext`].
    runtime: Runtime,
}

impl fmt::Debug for ZkSyncNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        let self_ = Self {
            resource_provider: Box::new(resource_provider),
            resources: RefCell::default(),
            task_constructors: Vec::new(),
            healthcheck_task_constructor: None,
            stop_sender,
            stop_receiver: StopReceiver(stop_receiver),
            runtime,
        };

        Ok(self_)
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

    /// Returns `true` if the healthcheck task is already set.
    pub fn has_healthcheck_task(&self) -> bool {
        self.healthcheck_task_constructor.is_some()
    }

    /// Adds a healthcheck task to the node.
    ///
    /// Healthcheck task is treated as any other task, with the following differences:
    /// - It is given the list of healthchecks of all the other tasks.
    /// - Its own healthcheck is ignored (the task is expected to handle its own checks itself).
    /// - There may be only one healthcheck task per node.
    ///
    /// If the healthcheck task is already set, the provided one will override it.
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
            let healthcheck = task.healthcheck();
            let after_node_shutdown = task.after_node_shutdown();
            let task_future = Box::pin(task.run(self.stop_receiver.clone()));
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
            let task_future = Box::pin(healthcheck_task.run(self.stop_receiver.clone()));
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
        // TODO (QIT-24): wrap every task into a timeout to prevent hanging.
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
