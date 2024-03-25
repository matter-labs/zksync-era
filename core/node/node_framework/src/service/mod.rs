use std::{collections::HashMap, time::Duration};

use anyhow::Context;
use futures::{future::BoxFuture, FutureExt};
use tokio::{runtime::Runtime, sync::watch};
use zksync_utils::panic_extractor::try_extract_panic_message;

use self::runnables::Runnables;
pub use self::{context::ServiceContext, error::ZkStackServiceError, stop_receiver::StopReceiver};
use crate::{
    resource::{ResourceId, StoredResource},
    service::runnables::TaskReprs,
    wiring_layer::{WiringError, WiringLayer},
};

mod context;
mod error;
mod runnables;
mod stop_receiver;
#[cfg(test)]
mod tests;

// A reasonable amount of time for any task to finish the shutdown process
const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// A builder for [`ZkStackService`].
#[derive(Default, Debug)]
pub struct ZkStackServiceBuilder {
    /// List of wiring layers.
    layers: Vec<Box<dyn WiringLayer>>,
}

impl ZkStackServiceBuilder {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Adds a wiring layer.
    /// During the [`run`](ZkStackService::run) call the service will invoke
    /// `wire` method of every layer in the order they were added.
    ///
    /// This method may be invoked multiple times with the same layer type, but the
    /// layer will only be stored once (meaning that 2nd attempt to add the same layer will be ignored).
    /// This may be useful if the same layer is a prerequisite for multiple other layers: it is safe
    /// to add it multiple times, and it will only be wired once.
    pub fn add_layer<T: WiringLayer>(&mut self, layer: T) -> &mut Self {
        if !self
            .layers
            .iter()
            .any(|existing_layer| existing_layer.layer_name() == layer.layer_name())
        {
            self.layers.push(Box::new(layer));
        }
        self
    }

    pub fn build(&mut self) -> Result<ZkStackService, ZkStackServiceError> {
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(ZkStackServiceError::RuntimeDetected);
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let (stop_sender, _stop_receiver) = watch::channel(false);

        Ok(ZkStackService {
            layers: std::mem::take(&mut self.layers),
            resources: Default::default(),
            runnables: Default::default(),
            stop_sender,
            runtime,
        })
    }
}

/// "Manager" class for a set of tasks. Collects all the resources and tasks,
/// then runs tasks until completion.
#[derive(Debug)]
pub struct ZkStackService {
    /// Cache of resources that have been requested at least by one task.
    resources: HashMap<ResourceId, Box<dyn StoredResource>>,
    /// List of wiring layers.
    layers: Vec<Box<dyn WiringLayer>>,
    /// Different kinds of tasks for the service.
    runnables: Runnables,

    /// Sender used to stop the tasks.
    stop_sender: watch::Sender<bool>,
    /// Tokio runtime used to spawn tasks.
    runtime: Runtime,
}

impl ZkStackService {
    /// Runs the system.
    pub fn run(mut self) -> Result<(), ZkStackServiceError> {
        // Initialize tasks.
        let wiring_layers = std::mem::take(&mut self.layers);

        let mut errors: Vec<(String, WiringError)> = Vec::new();

        let runtime_handle = self.runtime.handle().clone();
        for layer in wiring_layers {
            let name = layer.layer_name().to_string();
            // We must process wiring layers sequentially and in the same order as they were added.
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
            for (layer, error) in &errors {
                tracing::error!("Wiring layer {layer} can't be initialized: {error}");
            }
            return Err(ZkStackServiceError::Wiring(errors));
        }

        if self.runnables.is_empty() {
            return Err(ZkStackServiceError::NoTasks);
        }

        let only_oneshot_tasks = self.runnables.is_oneshot_only();

        // Barrier that will only be lifted once all the preconditions are met.
        // It will be awaited by the tasks before they start running and by the preconditions once they are fulfilled.
        let task_barrier = self.runnables.task_barrier();

        // Collect long-running tasks.
        let stop_receiver = StopReceiver(self.stop_sender.subscribe());
        let TaskReprs {
            mut long_running_tasks,
            oneshot_tasks,
        } = self
            .runnables
            .prepare_tasks(task_barrier.clone(), stop_receiver.clone());

        // Wiring is now complete.
        for resource in self.resources.values_mut() {
            resource.stored_resource_wired();
        }
        tracing::info!("Wiring complete");

        // Create a system task that is cancellation-aware and will only exit on either oneshot task failure or
        // stop signal.
        let oneshot_runner_system_task =
            oneshot_runner_task(oneshot_tasks, stop_receiver, only_oneshot_tasks);
        long_running_tasks.push(oneshot_runner_system_task);

        // Prepare tasks for running.
        let rt_handle = self.runtime.handle().clone();
        let join_handles: Vec<_> = long_running_tasks
            .into_iter()
            .map(|task| rt_handle.spawn(task).fuse())
            .collect();

        // Run the tasks until one of them exits.
        let (resolved, _, remaining) = self
            .runtime
            .block_on(futures::future::select_all(join_handles));
        let result = match resolved {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => Err(err).context("Task failed"),
            Err(panic_err) => {
                let panic_msg = try_extract_panic_message(panic_err);
                Err(anyhow::format_err!(
                    "One of the tasks panicked: {panic_msg}"
                ))
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

        result?;
        Ok(())
    }
}

fn oneshot_runner_task(
    oneshot_tasks: Vec<BoxFuture<'static, anyhow::Result<()>>>,
    mut stop_receiver: StopReceiver,
    only_oneshot_tasks: bool,
) -> BoxFuture<'static, anyhow::Result<()>> {
    Box::pin(async move {
        let oneshot_tasks = oneshot_tasks.into_iter().map(|fut| async move {
            // Spawn each oneshot task as a separate tokio task.
            // This way we can handle the cases when such a task panics and propagate the message
            // to the service.
            let handle = tokio::runtime::Handle::current();
            match handle.spawn(fut).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(err),
                Err(panic_err) => {
                    let panic_msg = try_extract_panic_message(panic_err);
                    Err(anyhow::format_err!("Oneshot task panicked: {panic_msg}"))
                }
            }
        });

        match futures::future::try_join_all(oneshot_tasks).await {
            Err(err) => Err(err),
            Ok(_) if only_oneshot_tasks => {
                // We only run oneshot tasks in this service, so we can exit now.
                Ok(())
            }
            Ok(_) => {
                // All oneshot tasks have exited and we have at least one long-running task.
                // Simply wait for the stop signal.
                stop_receiver.0.changed().await.ok();
                Ok(())
            }
        }
        // Note that we don't have to `select` on the stop signal explicitly:
        // Each prerequisite is given a stop signal, and if everyone respects it, this future
        // will still resolve once the stop signal is received.
    })
}
