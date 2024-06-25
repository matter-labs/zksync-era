use std::{fmt, sync::Arc};

use futures::future::BoxFuture;
use tokio::sync::Barrier;

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
    pub(super) long_running_tasks: Vec<NamedBoxFuture<anyhow::Result<()>>>,
    pub(super) oneshot_tasks: Vec<NamedBoxFuture<anyhow::Result<()>>>,
    pub(super) shutdown_hooks: Vec<NamedBoxFuture<anyhow::Result<()>>>,
}

impl fmt::Debug for TaskReprs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskReprs")
            .field("long_running_tasks", &self.long_running_tasks.len())
            .field("oneshot_tasks", &self.oneshot_tasks.len())
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

    /// Returns `true` if there are no long-running tasks in the collection.
    pub(super) fn is_oneshot_only(&self) -> bool {
        self.tasks.iter().all(|t| t.kind().is_oneshot())
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
        mut self,
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

        TaskReprs {
            long_running_tasks,
            oneshot_tasks,
            shutdown_hooks: self.shutdown_hooks,
        }
    }
}
