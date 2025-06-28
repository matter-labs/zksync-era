use std::{collections::HashMap, time::Duration};

use futures::future::Fuse;
use tokio::{runtime::Runtime, sync::watch, task::JoinHandle};
use zksync_utils::panic_extractor::try_extract_panic_message;

pub use self::{
    context::ServiceContext,
    context_traits::{FromContext, IntoContext},
    error::{TaskError, ZkStackServiceError},
    shutdown_hook::ShutdownHook,
    stop_receiver::StopReceiver,
};
use crate::{
    resource::{ResourceId, StoredResource},
    service::{
        named_future::NamedFuture,
        runnables::{NamedBoxFuture, Runnables, TaskReprs},
    },
    task::TaskId,
    wiring_layer::{WireFn, WiringError, WiringLayer, WiringLayerExt},
};

mod context;
mod context_traits;
mod error;
mod named_future;
mod runnables;
mod shutdown_hook;
mod stop_receiver;
#[cfg(test)]
mod tests;

// A reasonable amount of time for any task to finish the shutdown process
const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// A builder for [`ZkStackService`].
#[derive(Debug)]
pub struct ZkStackServiceBuilder {
    /// List of wiring layers.
    // Note: It has to be a `Vec` and not e.g. `HashMap` because the order in which we
    // iterate through it matters.
    layers: Vec<(&'static str, WireFn)>,
    /// Tokio runtime used to spawn tasks.
    runtime: Runtime,
}

impl ZkStackServiceBuilder {
    /// Creates a new builder.
    ///
    /// Returns an error if called within a Tokio runtime context.
    pub fn new() -> Result<Self, ZkStackServiceError> {
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(ZkStackServiceError::RuntimeDetected);
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        Ok(Self::on_runtime(runtime))
    }

    /// Creates a new builder with the provided Tokio runtime.
    /// This method can be used if asynchronous tasks must be performed before the service is built.
    ///
    /// However, it is not recommended to use this method to spawn any tasks that will not be managed
    /// by the service itself, so whenever it can be avoided, using [`ZkStackServiceBuilder::new`] is preferred.
    pub fn on_runtime(runtime: Runtime) -> Self {
        Self {
            layers: Vec::new(),
            runtime,
        }
    }

    /// Returns a handle to the Tokio runtime used by the service.
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    /// Adds a wiring layer.
    ///
    /// During the [`run`](ZkStackService::run) call the service will invoke
    /// `wire` method of every layer in the order they were added.
    ///
    /// This method may be invoked multiple times with the same layer type, but the
    /// layer will only be stored once (meaning that 2nd attempt to add the same layer will be ignored).
    /// This may be useful if the same layer is a prerequisite for multiple other layers: it is safe
    /// to add it multiple times, and it will only be wired once.
    pub fn add_layer<T: WiringLayer>(&mut self, layer: T) -> &mut Self {
        let name = layer.layer_name();
        if !self
            .layers
            .iter()
            .any(|(existing_name, _)| name == *existing_name)
        {
            self.layers.push((name, layer.into_wire_fn()));
        }
        self
    }

    /// Builds the service.
    pub fn build(self) -> ZkStackService {
        let (stop_sender, _stop_receiver) = watch::channel(false);

        ZkStackService {
            layers: self.layers,
            resources: Default::default(),
            runnables: Default::default(),
            stop_sender,
            runtime: self.runtime,
            errors: Vec::new(),
        }
    }
}

/// "Manager" class for a set of tasks. Collects all the resources and tasks,
/// then runs tasks until completion.
#[derive(Debug)]
pub struct ZkStackService {
    /// Cache of resources that have been requested at least by one task.
    resources: HashMap<ResourceId, Box<dyn StoredResource>>,
    /// List of wiring layers.
    layers: Vec<(&'static str, WireFn)>,
    /// Different kinds of tasks for the service.
    runnables: Runnables,

    /// Sender used to stop the tasks.
    stop_sender: watch::Sender<bool>,
    /// Tokio runtime used to spawn tasks.
    runtime: Runtime,

    /// Collector for the task errors met during the service execution.
    errors: Vec<TaskError>,
}

type TaskFuture = NamedFuture<Fuse<JoinHandle<anyhow::Result<()>>>>;

impl ZkStackService {
    /// Runs the system.
    ///
    /// In case of errors during wiring phase, will return the list of all the errors that happened, in the order
    /// of their occurrence.
    ///
    /// `observability_guard`, if provided, will be used to deinitialize the observability subsystem
    /// as the very last step before exiting the node.
    pub fn run<G>(mut self, observability_guard: G) -> Result<(), ZkStackServiceError> {
        self.wire()?;

        let TaskReprs {
            tasks,
            shutdown_hooks,
        } = self.prepare_tasks();

        let remaining = self.run_tasks(tasks);
        self.shutdown_tasks(remaining);
        self.run_shutdown_hooks(shutdown_hooks);

        tracing::info!("Exiting the service");

        {
            // Make sure that the shutdown happens in the `tokio` context.
            let _rt_guard = self.runtime.enter();
            drop(observability_guard);
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ZkStackServiceError::Task(self.errors.into()))
        }
    }

    /// Performs wiring of the service.
    /// After invoking this method, the collected tasks will be collected in `self.runnables`.
    fn wire(&mut self) -> Result<(), ZkStackServiceError> {
        // Initialize tasks.
        let wiring_layers = std::mem::take(&mut self.layers);

        let mut errors: Vec<(String, WiringError)> = Vec::new();

        let runtime_handle = self.runtime.handle().clone();
        for (name, WireFn(wire_fn)) in wiring_layers {
            // We must process wiring layers sequentially and in the same order as they were added.
            let mut context = ServiceContext::new(name, self);
            let task_result = wire_fn(&runtime_handle, &mut context);
            if let Err(err) = task_result {
                // We don't want to bail on the first error, since it'll provide worse DevEx:
                // People likely want to fix as much problems as they can in one go, rather than have
                // to fix them one by one.
                errors.push((name.to_string(), err));
                continue;
            };
        }

        // Report all the errors we've met during the init.
        if !errors.is_empty() {
            for (layer, error) in &errors {
                tracing::error!("Wiring layer {layer} can't be initialized: {error:?}");
            }
            return Err(ZkStackServiceError::Wiring(errors));
        }

        if self.runnables.is_empty() {
            return Err(ZkStackServiceError::NoTasks);
        }

        // Wiring is now complete.
        self.resources = HashMap::default(); // Decrement reference counters for resources.
        tracing::info!("Wiring complete");

        Ok(())
    }

    /// Prepares collected tasks for running.
    fn prepare_tasks(&mut self) -> TaskReprs {
        // Barrier that will only be lifted once all the preconditions are met.
        // It will be awaited by the tasks before they start running and by the preconditions once they are fulfilled.
        let task_barrier = self.runnables.task_barrier();

        // Collect long-running tasks.
        let stop_receiver = StopReceiver(self.stop_sender.subscribe());
        self.runnables
            .prepare_tasks(task_barrier.clone(), stop_receiver.clone())
    }

    /// Spawn the provided tasks and runs them until at least one task exits, and returns the list
    /// of remaining tasks.
    /// Adds error, if any, to the `errors` vector.
    fn run_tasks(&mut self, tasks: Vec<NamedBoxFuture<anyhow::Result<()>>>) -> Vec<TaskFuture> {
        // Prepare tasks for running.
        let rt_handle = self.runtime.handle().clone();
        let join_handles: Vec<_> = tasks
            .into_iter()
            .map(|task| task.spawn(&rt_handle).fuse())
            .collect();

        // Collect names for remaining tasks for reporting purposes.
        let mut tasks_names: Vec<_> = join_handles.iter().map(|task| task.id()).collect();

        // Run the tasks until one of them exits.
        let (resolved, resolved_idx, remaining) = self
            .runtime
            .block_on(futures::future::select_all(join_handles));
        // Extract the result and report it to logs early, before waiting for any other task to shutdown.
        // We will also collect the errors from the remaining tasks, hence a vector.
        let task_name = tasks_names.swap_remove(resolved_idx);
        self.handle_task_exit(resolved, task_name);
        tracing::info!("One of the task has exited, shutting down the node");

        remaining
    }

    /// Sends the stop request and waits for the remaining tasks to finish.
    fn shutdown_tasks(&mut self, remaining: Vec<TaskFuture>) {
        // Send a stop request to remaining tasks and wait for them to finish.
        self.stop_sender.send(true).ok();

        // Collect names for remaining tasks for reporting purposes.
        // We have to re-collect, becuase `select_all` does not guarantes the order of returned remaining futures.
        let remaining_tasks_names: Vec<_> = remaining.iter().map(|task| task.id()).collect();
        let remaining_tasks_with_timeout: Vec<_> = remaining
            .into_iter()
            .map(|task| async { tokio::time::timeout(TASK_SHUTDOWN_TIMEOUT, task).await })
            .collect();

        let execution_results = self
            .runtime
            .block_on(futures::future::join_all(remaining_tasks_with_timeout));

        // Report the results of the remaining tasks.
        for (name, result) in remaining_tasks_names.into_iter().zip(execution_results) {
            match result {
                Ok(resolved) => {
                    self.handle_task_exit(resolved, name);
                }
                Err(_) => {
                    tracing::error!("Task {name} timed out");
                    self.errors.push(TaskError::TaskShutdownTimedOut(name));
                }
            }
        }
    }

    /// Runs the provided shutdown hooks.
    fn run_shutdown_hooks(&mut self, shutdown_hooks: Vec<NamedBoxFuture<anyhow::Result<()>>>) {
        // Run shutdown hooks sequentially.
        for hook in shutdown_hooks {
            let name = hook.id().clone();
            // Limit each shutdown hook to the same timeout as the tasks.
            let hook_with_timeout =
                async move { tokio::time::timeout(TASK_SHUTDOWN_TIMEOUT, hook).await };
            match self.runtime.block_on(hook_with_timeout) {
                Ok(Ok(())) => {
                    tracing::info!("Shutdown hook {name} completed");
                }
                Ok(Err(err)) => {
                    tracing::error!("Shutdown hook {name} failed: {err:?}");
                    self.errors.push(TaskError::ShutdownHookFailed(name, err));
                }
                Err(_) => {
                    tracing::error!("Shutdown hook {name} timed out");
                    self.errors.push(TaskError::ShutdownHookTimedOut(name));
                }
            }
        }
    }

    /// Checks the result of the task execution, logs the result, and stores the error if any.
    fn handle_task_exit(
        &mut self,
        task_result: Result<anyhow::Result<()>, tokio::task::JoinError>,
        task_name: TaskId,
    ) {
        match task_result {
            Ok(Ok(())) => {
                tracing::info!("Task {task_name} finished");
            }
            Ok(Err(err)) => {
                tracing::error!("Task {task_name} failed: {err:?}");
                self.errors.push(TaskError::TaskFailed(task_name, err));
            }
            Err(panic_err) => {
                let panic_msg = try_extract_panic_message(panic_err);
                tracing::error!("Task {task_name} panicked: {panic_msg}");
                self.errors
                    .push(TaskError::TaskPanicked(task_name, panic_msg));
            }
        };
    }
}
