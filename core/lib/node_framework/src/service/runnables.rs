use std::{fmt, sync::Arc};

use anyhow::Context as _;
use futures::{future::BoxFuture, FutureExt as _};
use tokio::sync::Barrier;
use zksync_utils::panic_extractor::try_extract_panic_message;

use super::{named_future::NamedFuture, StopReceiver};
use crate::task::{Task, TaskKind};

/// Alias for futures with the name assigned.
pub(crate) type NamedBoxFuture<T> = NamedFuture<BoxFuture<'static, T>>;

/// A collection of different flavors of tasks.
#[derive(Default)]
pub(super) struct Runnables {
    /// Tasks added to the service.
    pub(super) tasks: Vec<Box<dyn Task>>,
    /// List of hooks to be invoked after node shutdown.
    pub(super) shutdown_hooks: Vec<NamedBoxFuture<anyhow::Result<()>>>,
}

impl fmt::Debug for Runnables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runnables")
            .field("tasks", &self.tasks)
            .field("shutdown_hooks", &self.shutdown_hooks)
            .finish()
    }
}

/// A unified representation of tasks that can be run by the service.
pub(super) struct TaskReprs {
    pub(super) tasks: Vec<NamedBoxFuture<anyhow::Result<()>>>,
    pub(super) shutdown_hooks: Vec<NamedBoxFuture<anyhow::Result<()>>>,
}

impl fmt::Debug for TaskReprs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskReprs")
            .field("long_running_tasks", &self.tasks.len())
            .field("shutdown_hooks", &self.shutdown_hooks.len())
            .finish()
    }
}

impl Runnables {
    /// Returns `true` if there are no tasks in the collection.
    /// Preconditions are not considered tasks.
    pub(super) fn is_empty(&self) -> bool {
        // We don't consider preconditions to be tasks.
        self.tasks.is_empty()
    }

    /// Prepares a barrier that should be shared between tasks and preconditions.
    /// The barrier is configured to wait for all the participants to be ready.
    /// Barrier does not assume the existence of unconstrained tasks.
    pub(super) fn task_barrier(&self) -> Arc<Barrier> {
        let barrier_size = self
            .tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.kind(),
                    TaskKind::Precondition | TaskKind::OneshotTask | TaskKind::Task
                )
            })
            .count();
        Arc::new(Barrier::new(barrier_size))
    }

    /// Transforms the collection of tasks into a set of universal futures.
    pub(super) fn prepare_tasks(
        &mut self,
        task_barrier: Arc<Barrier>,
        stop_receiver: StopReceiver,
    ) -> TaskReprs {
        let mut long_running_tasks = Vec::new();
        let mut oneshot_tasks = Vec::new();

        for task in std::mem::take(&mut self.tasks) {
            let name = task.id();
            let kind = task.kind();
            let stop_receiver = stop_receiver.clone();
            let task_barrier = task_barrier.clone();
            let task_future: BoxFuture<'static, _> =
                Box::pin(task.run_internal(stop_receiver, task_barrier));
            let named_future = NamedFuture::new(task_future, name);
            if kind.is_oneshot() {
                oneshot_tasks.push(named_future);
            } else {
                long_running_tasks.push(named_future);
            }
        }

        let only_oneshot_tasks = long_running_tasks.is_empty();
        // Create a system task that is cancellation-aware and will only exit on either oneshot task failure or
        // a stop request.
        let oneshot_runner_system_task =
            oneshot_runner_task(oneshot_tasks, stop_receiver, only_oneshot_tasks);
        long_running_tasks.push(oneshot_runner_system_task);

        TaskReprs {
            tasks: long_running_tasks,
            shutdown_hooks: std::mem::take(&mut self.shutdown_hooks),
        }
    }
}

fn oneshot_runner_task(
    oneshot_tasks: Vec<NamedBoxFuture<anyhow::Result<()>>>,
    mut stop_receiver: StopReceiver,
    only_oneshot_tasks: bool,
) -> NamedBoxFuture<anyhow::Result<()>> {
    let future = async move {
        let oneshot_tasks = oneshot_tasks.into_iter().map(|fut| async move {
            // Spawn each oneshot task as a separate tokio task.
            // This way we can handle the cases when such a task panics and propagate the message
            // to the service.
            let handle = tokio::runtime::Handle::current();
            let name = fut.id().to_string();
            match handle.spawn(fut).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(err).with_context(|| format!("Oneshot task {name} failed")),
                Err(panic_err) => {
                    let panic_msg = try_extract_panic_message(panic_err);
                    Err(anyhow::format_err!(
                        "Oneshot task {name} panicked: {panic_msg}"
                    ))
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
                // All oneshot tasks have exited, and we have at least one long-running task.
                // Simply wait for the stop request.
                stop_receiver.0.changed().await.ok();
                Ok(())
            }
        }
        // Note that we don't have to `select` on the stop request explicitly:
        // Each prerequisite is given a stop request, and if everyone respects it, this future
        // will still resolve once the stop request is received.
    };

    NamedBoxFuture::new(future.boxed(), "oneshot_runner".into())
}
