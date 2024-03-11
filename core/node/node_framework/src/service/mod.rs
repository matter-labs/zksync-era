use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

use anyhow::Context;
use futures::{future::BoxFuture, FutureExt};
use tokio::{
    runtime::Runtime,
    sync::{oneshot, watch, Barrier},
};

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

        // Barrier that will only be lifted once all the preconditions are met.
        // It will be awaited by the tasks before they start running and by the preconditions once they are fulfilled.
        let task_barrier = Arc::new(Barrier::new(self.tasks.len() + self.preconditions.len()));
        let mut tasks: Vec<BoxFuture<'static, anyhow::Result<()>>> = Vec::new();
        let mut preconditions = Vec::new();
        // Add all the unconstrained tasks.
        for task in std::mem::take(&mut self.unconstrained_tasks) {
            let name = task.name().to_string();
            let stop_receiver = self.stop_receiver();
            let task_future = Box::pin(async move {
                task.run_unconstrained(stop_receiver)
                    .await
                    .with_context(|| format!("Task {name} failed"))
            });
            tasks.push(task_future);
        }
        // Add all the "normal" tasks.
        for task in std::mem::take(&mut self.tasks) {
            let name = task.name().to_string();
            let stop_receiver = self.stop_receiver();
            let task_barrier = task_barrier.clone();
            let task_future = Box::pin(async move {
                task.run_with_barrier(stop_receiver, task_barrier)
                    .await
                    .with_context(|| format!("Task {name} failed"))
            });
            tasks.push(task_future);
        }
        if tasks.is_empty() {
            anyhow::bail!("No tasks to run");
        }
        // Finally, the preconditions are just futures.
        for precondition in std::mem::take(&mut self.preconditions) {
            let name = precondition.name().to_string();
            let stop_receiver = self.stop_receiver();
            let task_barrier = task_barrier.clone();
            let task_future = Box::pin(async move {
                precondition
                    .check_with_barrier(stop_receiver, task_barrier)
                    .await
                    .with_context(|| format!("Precondition {name} failed"))
            });
            // Internally we use the same representation for tasks and preconditions.
            preconditions.push(task_future);
        }

        // Wiring is now complete.
        for resource in self.resources.values_mut() {
            resource.stored_resource_wired();
        }

        // Launch precondition checkers. Generally, if all of them succeed, we don't need to do anything.
        // If any of them fails, the node have to shut down.
        // So we run them in a detached task with an oneshot channel to report the error, if it occurs,
        // and we'll use this channel in the same way that we use to check if any of the task exits.
        // The difference is that preconditions, unlike tasks, are allowed to exit without causing the node
        // to shut down.
        let (precondition_fail_sender, precondition_fail_receiver) = oneshot::channel();
        self.runtime.spawn(async move {
            let preconditions = preconditions.into_iter().map(|fut| async move {
                // Spawn each precondition as a separate task.
                // This way we can handle the cases when a precondition task panics and propagate the message
                // to the service.
                // This is important since this task is detached: we do not store a handle for it and never await it explicitly.
                let handle = tokio::runtime::Handle::current();
                match handle.spawn(fut).await {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(err)) => Err(err),
                    Err(_panic_err) => Err(anyhow::anyhow!("Precondition panicked")), // TODO: Extract message
                }
            });

            if let Err(err) = futures::future::try_join_all(preconditions).await {
                precondition_fail_sender.send(err).ok();
            }
        });
        // Now we can create a system task that is cancellation-aware and will only exit on either
        // precondition failure or stop signal.
        let mut stop_receiver = self.stop_receiver();
        let precondition_system_task = Box::pin(async move {
            match precondition_fail_receiver.await {
                Ok(err) => Err(err),
                Err(_) => {
                    // All preconditions successfully resolved and corresponding sender was dropped.
                    // Simply wait for the stop signal.
                    stop_receiver.0.changed().await.ok();
                    Ok(())
                }
            }
            // Note that we don't have to `select` on the stop signal explicitly:
            // Each prerequisite is given a stop signal, and if everyone respects it, this future
            // will still resolve once the stop signal is received.
        });
        tasks.push(precondition_system_task);

        // Prepare tasks for running.
        let rt_handle = self.runtime.handle().clone();
        let join_handles: Vec<_> = tasks
            .into_iter()
            .map(|task| rt_handle.spawn(task).fuse())
            .collect();

        // Run the tasks until one of them exits.
        let (resolved, _, remaining) = self
            .runtime
            .block_on(futures::future::select_all(join_handles));
        let failure = match resolved {
            Ok(Ok(())) => false,
            Ok(Err(err)) => {
                tracing::error!("One of the tasks exited with an error: {err}"); // TODO
                true
            }
            Err(_panic_msg) => {
                // tracing::error!("Task {resolved_task_name} panicked");  TODO
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
            // anyhow::bail!("Task {resolved_task_name} failed");
            // TODO
            anyhow::bail!("Task failed");
        } else {
            Ok(())
        }
    }

    pub(crate) fn stop_receiver(&self) -> StopReceiver {
        StopReceiver(self.stop_sender.subscribe())
    }
}
