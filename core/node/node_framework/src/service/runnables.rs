use std::{fmt, sync::Arc};

use anyhow::Context as _;
use futures::future::BoxFuture;
use tokio::sync::Barrier;

use super::StopReceiver;
use crate::{
    precondition::Precondition,
    task::{OneshotTask, Task, TaskId, UnconstrainedOneshotTask, UnconstrainedTask},
};

/// Alias for a shutdown hook function type.
pub trait ShutdownHookFn:
    FnOnce() -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync + 'static
{
}

impl<T> ShutdownHookFn for T where
    T: FnOnce() -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync + 'static
{
}

pub struct ShutdownHook {
    id: TaskId,
    hook: Box<dyn ShutdownHookFn>,
}

impl ShutdownHook {
    pub fn new(id: impl Into<TaskId>, hook: impl ShutdownHookFn) -> Self {
        Self {
            id: id.into(),
            hook: Box::new(hook),
        }
    }

    pub fn id(&self) -> &TaskId {
        &self.id
    }

    pub async fn invoke(self) -> anyhow::Result<()> {
        (self.hook)().await
    }
}

impl fmt::Debug for ShutdownHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShutdownHook")
            .field("name", &self.id)
            .finish()
    }
}

/// A collection of different flavors of tasks.
#[derive(Default)]
pub(super) struct Runnables {
    /// Preconditions added to the service.
    pub(super) preconditions: Vec<Box<dyn Precondition>>,
    /// Tasks added to the service.
    pub(super) tasks: Vec<Box<dyn Task>>,
    /// Oneshot tasks added to the service.
    pub(super) oneshot_tasks: Vec<Box<dyn OneshotTask>>,
    /// Unconstrained tasks added to the service.
    pub(super) unconstrained_tasks: Vec<Box<dyn UnconstrainedTask>>,
    /// Unconstrained oneshot tasks added to the service.
    pub(super) unconstrained_oneshot_tasks: Vec<Box<dyn UnconstrainedOneshotTask>>,
    /// List of hooks to be invoked after node shutdown.
    pub(super) shutdown_hooks: Vec<ShutdownHook>,
}

impl fmt::Debug for Runnables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runnables")
            .field("preconditions", &self.preconditions)
            .field("tasks", &self.tasks)
            .field("oneshot_tasks", &self.oneshot_tasks)
            .field("unconstrained_tasks", &self.unconstrained_tasks)
            .field(
                "unconstrained_oneshot_tasks",
                &self.unconstrained_oneshot_tasks,
            )
            .field("shutdown_hooks", &self.shutdown_hooks)
            .finish()
    }
}

/// A unified representation of tasks that can be run by the service.
pub(super) struct TaskReprs {
    pub(super) long_running_tasks: Vec<BoxFuture<'static, anyhow::Result<()>>>,
    pub(super) oneshot_tasks: Vec<BoxFuture<'static, anyhow::Result<()>>>,
    pub(super) shutdown_hooks: Vec<ShutdownHook>,
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
            && self.oneshot_tasks.is_empty()
            && self.unconstrained_tasks.is_empty()
            && self.unconstrained_oneshot_tasks.is_empty()
    }

    /// Returns `true` if there are no long-running tasks in the collection.
    pub(super) fn is_oneshot_only(&self) -> bool {
        self.tasks.is_empty() && self.unconstrained_tasks.is_empty()
    }

    /// Prepares a barrier that should be shared between tasks and preconditions.
    /// The barrier is configured to wait for all the participants to be ready.
    /// Barrier does not assume the existence of unconstrained tasks.
    pub(super) fn task_barrier(&self) -> Arc<Barrier> {
        Arc::new(Barrier::new(
            self.tasks.len() + self.preconditions.len() + self.oneshot_tasks.len(),
        ))
    }

    /// Transforms the collection of tasks into a set of universal futures.
    pub(super) fn prepare_tasks(
        mut self,
        task_barrier: Arc<Barrier>,
        stop_receiver: StopReceiver,
    ) -> TaskReprs {
        let mut long_running_tasks = Vec::new();
        self.collect_unconstrained_tasks(&mut long_running_tasks, stop_receiver.clone());
        self.collect_tasks(
            &mut long_running_tasks,
            task_barrier.clone(),
            stop_receiver.clone(),
        );

        let mut oneshot_tasks = Vec::new();
        self.collect_preconditions(
            &mut oneshot_tasks,
            task_barrier.clone(),
            stop_receiver.clone(),
        );
        self.collect_oneshot_tasks(
            &mut oneshot_tasks,
            task_barrier.clone(),
            stop_receiver.clone(),
        );
        self.collect_unconstrained_oneshot_tasks(&mut oneshot_tasks, stop_receiver.clone());

        TaskReprs {
            long_running_tasks,
            oneshot_tasks,
            shutdown_hooks: self.shutdown_hooks,
        }
    }

    fn collect_unconstrained_tasks(
        &mut self,
        tasks: &mut Vec<BoxFuture<'static, anyhow::Result<()>>>,
        stop_receiver: StopReceiver,
    ) {
        for task in std::mem::take(&mut self.unconstrained_tasks) {
            let name = task.id();
            let stop_receiver = stop_receiver.clone();
            let task_future = Box::pin(async move {
                task.run_unconstrained(stop_receiver)
                    .await
                    .with_context(|| format!("Task {name} failed"))
            });
            tasks.push(task_future);
        }
    }

    fn collect_tasks(
        &mut self,
        tasks: &mut Vec<BoxFuture<'static, anyhow::Result<()>>>,
        task_barrier: Arc<Barrier>,
        stop_receiver: StopReceiver,
    ) {
        for task in std::mem::take(&mut self.tasks) {
            let name = task.id();
            let stop_receiver = stop_receiver.clone();
            let task_barrier = task_barrier.clone();
            let task_future = Box::pin(async move {
                task.run_with_barrier(stop_receiver, task_barrier)
                    .await
                    .with_context(|| format!("Task {name} failed"))
            });
            tasks.push(task_future);
        }
    }

    fn collect_preconditions(
        &mut self,
        oneshot_tasks: &mut Vec<BoxFuture<'static, anyhow::Result<()>>>,
        task_barrier: Arc<Barrier>,
        stop_receiver: StopReceiver,
    ) {
        for precondition in std::mem::take(&mut self.preconditions) {
            let name = precondition.id();
            let stop_receiver = stop_receiver.clone();
            let task_barrier = task_barrier.clone();
            let task_future = Box::pin(async move {
                precondition
                    .check_with_barrier(stop_receiver, task_barrier)
                    .await
                    .with_context(|| format!("Precondition {name} failed"))
            });
            oneshot_tasks.push(task_future);
        }
    }

    fn collect_oneshot_tasks(
        &mut self,
        oneshot_tasks: &mut Vec<BoxFuture<'static, anyhow::Result<()>>>,
        task_barrier: Arc<Barrier>,
        stop_receiver: StopReceiver,
    ) {
        for oneshot_task in std::mem::take(&mut self.oneshot_tasks) {
            let name = oneshot_task.id();
            let stop_receiver = stop_receiver.clone();
            let task_barrier = task_barrier.clone();
            let task_future = Box::pin(async move {
                oneshot_task
                    .run_oneshot_with_barrier(stop_receiver, task_barrier)
                    .await
                    .with_context(|| format!("Oneshot task {name} failed"))
            });
            oneshot_tasks.push(task_future);
        }
    }

    fn collect_unconstrained_oneshot_tasks(
        &mut self,
        oneshot_tasks: &mut Vec<BoxFuture<'static, anyhow::Result<()>>>,
        stop_receiver: StopReceiver,
    ) {
        for unconstrained_oneshot_task in std::mem::take(&mut self.unconstrained_oneshot_tasks) {
            let name = unconstrained_oneshot_task.id();
            let stop_receiver = stop_receiver.clone();
            let task_future = Box::pin(async move {
                unconstrained_oneshot_task
                    .run_unconstrained_oneshot(stop_receiver)
                    .await
                    .with_context(|| format!("Unconstrained oneshot task {name} failed"))
            });
            oneshot_tasks.push(task_future);
        }
    }
}
