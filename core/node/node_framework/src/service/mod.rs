use std::{collections::HashMap, fmt, time::Duration};

use futures::{future::BoxFuture, FutureExt};
use tokio::{runtime::Runtime, sync::watch};

pub use self::{context::ServiceContext, stop_receiver::StopReceiver};
use crate::{
    precondition::Precondition,
    resource::{ResourceId, StoredResource},
    task::{Task, UnconstrainedTask},
    wiring_layer::{WiringError, WiringLayer},
};

mod context;
mod stop_receiver;
#[cfg(test)]
mod tests;

// A reasonable amount of time for any task to finish the shutdown process
const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// "Manager" class for a set of tasks. Collects all the resources and tasks,
/// then runs tasks until completion.
///
/// Initialization flow:
/// - Service instance is created with access to the resource provider.
/// - Wiring layers are added to the service. At this step, tasks are not created yet.
/// - Once the `run` method is invoked, service
///   - invokes a `wire` method on each added wiring layer. If any of the layers fails,
///     the service will return an error. If no layers have added a task, the service will
///     also return an error.
///   - waits for any of the tasks to finish.
///   - sends stop signal to all the tasks.
///   - waits for the remaining tasks to finish.
///   - returns the result of the task that has finished.
pub struct ZkStackService {
    /// Cache of resources that have been requested at least by one task.
    resources: HashMap<ResourceId, Box<dyn StoredResource>>,
    /// List of wiring layers.
    layers: Vec<Box<dyn WiringLayer>>,
    /// Preconditions added to the service.
    preconditions: Vec<Box<dyn Precondition>>,
    /// Tasks added to the service.
    tasks: Vec<Box<dyn Task>>,
    /// Unconstrained tasks added to the service.
    unconstrained_tasks: Vec<Box<dyn UnconstrainedTask>>,

    /// Sender used to stop the tasks.
    stop_sender: watch::Sender<bool>,
    /// Tokio runtime used to spawn tasks.
    runtime: Runtime,
}

impl fmt::Debug for ZkStackService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZkStackService").finish_non_exhaustive()
    }
}

impl ZkStackService {
    pub fn new() -> anyhow::Result<Self> {
        if tokio::runtime::Handle::try_current().is_ok() {
            anyhow::bail!(
                "Detected a Tokio Runtime. ZkStackService manages its own runtime and does not support nested runtimes"
            );
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let (stop_sender, _stop_receiver) = watch::channel(false);
        let self_ = Self {
            resources: HashMap::default(),
            layers: Vec::new(),
            preconditions: Vec::new(),
            tasks: Vec::new(),
            unconstrained_tasks: Vec::new(),
            stop_sender,
            runtime,
        };

        Ok(self_)
    }

    /// Adds a wiring layer.
    /// During the [`run`](ZkStackService::run) call the service will invoke
    /// `wire` method of every layer in the order they were added.
    pub fn add_layer<T: WiringLayer>(&mut self, layer: T) -> &mut Self {
        self.layers.push(Box::new(layer));
        self
    }

    /// Runs the system.
    pub fn run(mut self) -> anyhow::Result<()> {
        // Initialize tasks.
        let wiring_layers = std::mem::take(&mut self.layers);

        let mut errors: Vec<(String, WiringError)> = Vec::new();

        let runtime_handle = self.runtime.handle().clone();
        for layer in wiring_layers {
            let name = layer.layer_name().to_string();
            let task_result =
                runtime_handle.block_on(layer.wire(ServiceContext::new(&name, &mut self)));
            if let Err(err) = task_result {
                // We don't want to bail on the first error, since it'll provide worse DevEx:
                // People likely want to fix as much problems as they can in one go, rather than have
                // to fix them one by one.
                errors.push((name, err));
                continue;
            };
        }

        // Report all the errors we've met during the init.
        if !errors.is_empty() {
            for (task, error) in errors {
                tracing::error!("Task {task} can't be initialized: {error}");
            }
            anyhow::bail!("One or more task weren't able to start");
        }

        let (preconditions_sender, preconditions_receiver) = watch::channel(false);
        let mut tasks = Vec::new();
        // Add all the unconstrained tasks.
        for task in std::mem::take(&mut self.unconstrained_tasks) {
            let name = task.name().to_string();
            let task_future = Box::pin(task.run_unconstrained(self.stop_receiver()));
            let task_repr = TaskRepr {
                name,
                task: Some(task_future),
            };
            tasks.push(task_repr);
        }
        // Add all the "normal" tasks.
        for task in std::mem::take(&mut self.tasks) {
            let name = task.name().to_string();
            let task_future = Box::pin(
                task.run_with_preconditions(self.stop_receiver(), preconditions_receiver.clone()),
            );
            let task_repr = TaskRepr {
                name,
                task: Some(task_future),
            };
            tasks.push(task_repr);
        }
        if tasks.is_empty() {
            anyhow::bail!("No tasks to run");
        }
        // Finally, the preconditions are handles the same way the tasks are handled.
        for precondition in std::mem::take(&mut self.preconditions) {
            let name = precondition.name().to_string();
            let task_future = Box::pin(precondition.check());
            let task_repr = TaskRepr {
                name,
                task: Some(task_future),
            };
            tasks.push(task_repr);
        }
        // TODO: how to signal that preconditions are checked?

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
        let (resolved, resolved_idx, remaining) = self
            .runtime
            .block_on(futures::future::select_all(join_handles));
        let resolved_task_name = tasks[resolved_idx].name.clone();
        let failure = match resolved {
            Ok(Ok(())) => {
                tracing::info!("Task {resolved_task_name} completed");
                false
            }
            Ok(Err(err)) => {
                tracing::error!("Task {resolved_task_name} exited with an error: {err}");
                true
            }
            Err(_) => {
                tracing::error!("Task {resolved_task_name} panicked");
                true
            }
        };

        let remaining_tasks_with_timeout: Vec<_> = remaining
            .into_iter()
            .map(|task| async { tokio::time::timeout(TASK_SHUTDOWN_TIMEOUT, task).await })
            .collect();

        // Send stop signal to remaining tasks and wait for them to finish.
        // Given that we are shutting down, we do not really care about returned values.
        self.stop_sender.send(true).ok();
        let execution_results = self
            .runtime
            .block_on(futures::future::join_all(remaining_tasks_with_timeout));
        let execution_timeouts_count = execution_results.iter().filter(|&r| r.is_err()).count();
        if execution_timeouts_count > 0 {
            tracing::warn!(
                "{execution_timeouts_count} tasks didn't finish in {TASK_SHUTDOWN_TIMEOUT:?} and were dropped"
            );
        } else {
            tracing::info!("Remaining tasks finished without reaching timeouts");
        }

        if failure {
            anyhow::bail!("Task {resolved_task_name} failed");
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
}

impl fmt::Debug for TaskRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskRepr")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
