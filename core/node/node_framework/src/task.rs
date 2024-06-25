//! Tasks define the "runnable" concept of the service, e.g. a unit of work that can be executed by the service.
//!
//! ## Task kinds
//!
//! This module defines different flavors of tasks.
//! The most basic one is [`Task`], which is only launched after all the preconditions are met (more on this later),
//! and is expected to run until the node is shut down. This is the most common type of task, e.g. API server,
//! state keeper, and metadata calculator are examples of such tasks.
//!
//! Then there exists an [`OneshotTask`], which has a clear exit condition that does not cause the node to shut down.
//! This is useful for tasks that are expected to run once and then exit, e.g. a task that performs a programmatic
//! migration.
//!
//! Finally, the task can be unconstrained by preconditions, which means that it will start immediately without
//! waiting for any preconditions to be met. This kind of tasks is represent by [`UnconstrainedTask`] and
//! [`UnconstrainedOneshotTask`].
//!
//! ## Tasks and preconditions
//!
//! Besides tasks, service also has a concept of preconditions(crate::precondition::Precondition). Precondition is a
//! piece of logic that is expected to be met before the task can start. One can think of preconditions as a way to
//! express invariants that the tasks may rely on.
//!
//! In this notion, the difference between a task and an unconstrained task is that the former has all the invariants
//! checked already, and unrestricted task is responsible for *manually checking any invariants it may rely on*.
//!
//! The unrestricted tasks are rarely needed, but two common cases for them are:
//! - A task that must be started as soon as possible, e.g. healthcheck server.
//! - A task that may be a driving force for some precondition to be met.

use std::{
    fmt::{self, Display, Formatter},
    ops::Deref,
    sync::Arc,
};

use tokio::sync::Barrier;

use crate::service::StopReceiver;

/// A unique human-readable identifier of a task.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(value: String) -> Self {
        TaskId(value)
    }
}

impl Display for TaskId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TaskId {
    fn from(value: &str) -> Self {
        TaskId(value.to_owned())
    }
}

impl From<String> for TaskId {
    fn from(value: String) -> Self {
        TaskId(value)
    }
}

impl Deref for TaskId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A task implementation.
///
/// Note: any `Task` added to the service will only start after all the [preconditions](crate::precondition::Precondition)
/// are met. If a task should start immediately, one should use [`UnconstrainedTask`](crate::task::UnconstrainedTask).
#[async_trait::async_trait]
pub trait Task: 'static + Send {
    /// Unique name of the task.
    fn id(&self) -> TaskId;

    /// Runs the task.
    ///
    /// Once any of the task returns, the node will shutdown.
    /// If the task returns an error, the node will spawn an error-level log message and will return a non-zero
    /// exit code.
    ///
    /// `stop_receiver` argument contains a channel receiver that will change its value once the node requests
    /// a shutdown. Every task is expected to either await or periodically check the state of channel and stop
    /// its execution once the channel is changed.
    ///
    /// Each task is expected to perform the required cleanup after receiving the stop signal.
    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()>;
}

impl dyn Task {
    /// An internal helper method that guards running the task with a tokio Barrier.
    /// Used to make sure that the task is not started until all the preconditions are met.
    pub(super) async fn run_with_barrier(
        self: Box<Self>,
        mut stop_receiver: StopReceiver,
        preconditions_barrier: Arc<Barrier>,
    ) -> anyhow::Result<()> {
        // Wait either for barrier to be lifted or for the stop signal to be received.
        tokio::select! {
            _ = preconditions_barrier.wait() => {
                self.run(stop_receiver).await
            }
            _ = stop_receiver.0.changed() => {
                Ok(())
            }
        }
    }
}

impl fmt::Debug for dyn Task {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Task").field("name", &self.id()).finish()
    }
}

/// A oneshot task implementation.
/// The difference from [`Task`] is that this kind of task may exit without causing the service to shutdown.
///
/// Note: any `Task` added to the service will only start after all the [preconditions](crate::precondition::Precondition)
/// are met. If a task should start immediately, one should use [`UnconstrainedTask`](crate::task::UnconstrainedTask).
#[async_trait::async_trait]
pub trait OneshotTask: 'static + Send {
    /// Unique name of the task.
    fn id(&self) -> TaskId;

    /// Runs the task.
    ///
    /// Unlike [`Task::run`], this method is expected to return once the task is finished, without causing the
    /// node to shutdown.
    ///
    /// `stop_receiver` argument contains a channel receiver that will change its value once the node requests
    /// a shutdown. Every task is expected to either await or periodically check the state of channel and stop
    /// its execution once the channel is changed.
    ///
    /// Each task is expected to perform the required cleanup after receiving the stop signal.
    async fn run_oneshot(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()>;
}

impl dyn OneshotTask {
    /// An internal helper method that guards running the task with a tokio Barrier.
    /// Used to make sure that the task is not started until all the preconditions are met.
    pub(super) async fn run_oneshot_with_barrier(
        self: Box<Self>,
        mut stop_receiver: StopReceiver,
        preconditions_barrier: Arc<Barrier>,
    ) -> anyhow::Result<()> {
        // Wait either for barrier to be lifted or for the stop signal to be received.
        tokio::select! {
            _ = preconditions_barrier.wait() => {
                self.run_oneshot(stop_receiver).await
            }
            _ = stop_receiver.0.changed() => {
                Ok(())
            }
        }
    }
}

impl fmt::Debug for dyn OneshotTask {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OneshotTask")
            .field("name", &self.id())
            .finish()
    }
}

/// A task implementation that is not constrained by preconditions.
///
/// This trait is used to define tasks that should start immediately after the wiring phase, without waiting for
/// any preconditions to be met.
///
/// *Warning*. An unconstrained task may not be aware of the state of the node and is expected to cautiously check
/// any invariants it may rely on.
#[async_trait::async_trait]
pub trait UnconstrainedTask: 'static + Send {
    /// Unique name of the task.
    fn id(&self) -> TaskId;

    /// Runs the task without waiting for any precondition to be met.
    async fn run_unconstrained(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()>;
}

impl fmt::Debug for dyn UnconstrainedTask {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnconstrainedTask")
            .field("name", &self.id())
            .finish()
    }
}

/// An unconstrained analog of [`OneshotTask`].
/// See [`UnconstrainedTask`] and [`OneshotTask`] for more details.
#[async_trait::async_trait]
pub trait UnconstrainedOneshotTask: 'static + Send {
    /// Unique name of the task.
    fn id(&self) -> TaskId;

    /// Runs the task without waiting for any precondition to be met.
    async fn run_unconstrained_oneshot(
        self: Box<Self>,
        stop_receiver: StopReceiver,
    ) -> anyhow::Result<()>;
}

impl fmt::Debug for dyn UnconstrainedOneshotTask {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnconstrainedOneshotTask")
            .field("name", &self.id())
            .finish()
    }
}
