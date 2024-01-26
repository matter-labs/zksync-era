use std::{collections::HashMap, fmt};

use futures::{future::BoxFuture, FutureExt};
use tokio::{runtime::Runtime, sync::watch};

pub use self::{context::NodeContext, stop_receiver::StopReceiver};
use crate::{
    resource::{ResourceId, ResourceProvider, StoredResource},
    task::{IntoZkSyncTask, TaskInitError},
};

mod context;
mod stop_receiver;

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
    resources: HashMap<ResourceId, Box<dyn StoredResource>>,
    /// List of task builders.
    task_builders: Vec<Box<dyn IntoZkSyncTask>>,

    /// Sender used to stop the tasks.
    stop_sender: watch::Sender<bool>,
    /// Tokio runtime used to spawn tasks.
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

        let (stop_sender, _stop_receiver) = watch::channel(false);
        let self_ = Self {
            resource_provider: Box::new(resource_provider),
            resources: HashMap::default(),
            task_builders: Vec::new(),
            stop_sender,
            runtime,
        };

        Ok(self_)
    }

    /// Adds a task to the node.
    ///
    /// The task is not created at this point, instead, the constructor is stored in the node
    /// and will be invoked during [`ZkSyncNode::run`] method. Any error returned by the constructor
    /// will prevent the node from starting and will be propagated by the [`ZkSyncNode::run`] method.
    pub fn add_task<T: IntoZkSyncTask>(&mut self, builder: T) -> &mut Self {
        self.task_builders.push(Box::new(builder));
        self
    }

    /// Runs the system.
    pub fn run(mut self) -> anyhow::Result<()> {
        // Initialize tasks.
        let task_builders = std::mem::take(&mut self.task_builders);

        let mut tasks = Vec::new();

        let mut errors: Vec<(String, TaskInitError)> = Vec::new();

        let runtime_handle = self.runtime.handle().clone();
        for task_builder in task_builders {
            let name = task_builder.task_name().to_string();
            let task_result =
                runtime_handle.block_on(task_builder.create(NodeContext::new(&mut self)));
            let task = match task_result {
                Ok(task) => task,
                Err(err) => {
                    // We don't want to bail on the first error, since it'll provide worse DevEx:
                    // People likely want to fix as much problems as they can in one go, rather than have
                    // to fix them one by one.
                    errors.push((name, err));
                    continue;
                }
            };
            let after_node_shutdown = task.after_node_shutdown();
            let task_future = Box::pin(task.run(self.stop_receiver()));
            let task_repr = TaskRepr {
                name,
                task: Some(task_future),
                after_node_shutdown,
            };
            tasks.push(task_repr);
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

        // Wiring is now complete.
        for resource in self.resources.values_mut() {
            resource.stored_resource_wired();
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

    pub(crate) fn stop_receiver(&self) -> StopReceiver {
        StopReceiver(self.stop_sender.subscribe())
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
