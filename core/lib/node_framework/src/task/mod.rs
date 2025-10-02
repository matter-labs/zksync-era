//! Tasks define the "runnable" concept of the service, e.g. a unit of work that can be executed by the service.

use std::{
    fmt::{self, Formatter},
    sync::Arc,
};

use tokio::sync::Barrier;

pub use self::types::{TaskId, TaskKind};
use crate::service::StopReceiver;

mod types;

/// A task implementation.
/// Task defines the "runnable" concept of the service, e.g. a unit of work that can be executed by the service.
///
/// Based on the task kind, the implemenation will be treated differently by the service.
///
/// ## Task kinds
///
/// There may be different kinds of tasks:
///
/// ### `Task`
///
/// A regular task. Returning from this task will cause the service to stop. [`Task::kind`] has a default
/// implementation that returns `TaskKind::Task`.
///
/// Typically, the implementation of [`Task::run`] will be some form of loop that runs until either an
/// irrecoverable error happens (then task should return an error), or a stop request is received (then task should
/// return `Ok(())`).
///
/// ### `OneshotTask`
///
/// A task that can exit when completed without causing the service to terminate.
/// In case of `OneshotTask`s, the service will only exit when all the `OneshotTask`s have exited and there are
/// no more tasks running.
///
/// ### `Precondition`
///
/// A "barrier" task that is supposed to check invariants before the main tasks are started.
/// An example of a precondition task could be a task that checks if the database has all the required data.
/// Precondition tasks are often paired with some other kind of task that will make sure that the precondition
/// can be satisfied. This is required for a distributed service setup, where the precondition task will be
/// present on all the nodes, while a task that satisfies the precondition will be present only on one node.
///
/// ### `UnconstrainedTask`
///
/// A task that can run without waiting for preconditions.
/// Tasks of this kind are expected to check all the invariants they rely on themselves.
/// Usually, this kind of task is used either for tasks that must start as early as possible (e.g. healthcheck server),
/// or for tasks that cannot rely on preconditions.
///
/// ### `UnconstrainedOneshotTask`
///
/// A task that can run without waiting for preconditions and can exit without stopping the service.
/// Usually such tasks may be used for satisfying a precondition, for example, they can perform the database
/// setup.
#[async_trait::async_trait]
pub trait Task: 'static + Send {
    /// Returns the kind of the task.
    /// The returned values is expected to be static, and it will be used by the service
    /// to determine how to handle the task.
    fn kind(&self) -> TaskKind {
        TaskKind::Task
    }

    /// Unique name of the task.
    fn id(&self) -> TaskId;

    /// Runs the task.
    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()>;
}

impl dyn Task {
    /// An internal helper method that guards running the task with a tokio Barrier.
    /// Used to make sure that the task is not started until all the preconditions are met.
    pub(super) async fn run_internal(
        self: Box<Self>,
        stop_receiver: StopReceiver,
        preconditions_barrier: Arc<Barrier>,
    ) -> anyhow::Result<()> {
        match self.kind() {
            TaskKind::Task | TaskKind::OneshotTask => {
                self.run_with_barrier(stop_receiver, preconditions_barrier)
                    .await
            }
            TaskKind::UnconstrainedTask | TaskKind::UnconstrainedOneshotTask => {
                self.run(stop_receiver).await
            }
            TaskKind::Precondition => {
                self.check_precondition(stop_receiver, preconditions_barrier)
                    .await
            }
        }
    }

    async fn run_with_barrier(
        self: Box<Self>,
        mut stop_receiver: StopReceiver,
        preconditions_barrier: Arc<Barrier>,
    ) -> anyhow::Result<()> {
        // Wait either for barrier to be lifted or for the stop request to be received.
        tokio::select! {
            _ = preconditions_barrier.wait() => {
                self.run(stop_receiver).await
            }
            _ = stop_receiver.0.changed() => {
                Ok(())
            }
        }
    }

    async fn check_precondition(
        self: Box<Self>,
        mut stop_receiver: StopReceiver,
        preconditions_barrier: Arc<Barrier>,
    ) -> anyhow::Result<()> {
        self.run(stop_receiver.clone()).await?;
        tokio::select! {
            _ = preconditions_barrier.wait() => {
                Ok(())
            }
            _ = stop_receiver.0.changed() => {
                Ok(())
            }
        }
    }
}

impl fmt::Debug for dyn Task {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Task")
            .field("kind", &self.kind())
            .field("name", &self.id())
            .finish()
    }
}
