//! Tasks define the "runnable" concept of the node, e.g. something that can be launched and runs until the node
//! is stopped.

use std::sync::Arc;

use tokio::sync::Barrier;

use crate::service::StopReceiver;

/// A task implementation.
///
/// Note: any `Task` added to the service will only start after all the [preconditions](crate::precondition::Precondition)
/// are met. If a task should start immediately, one should use [UnconstrainedTask](crate::task::UnconstrainedTask).
#[async_trait::async_trait]
pub trait Task: 'static + Send {
    /// Unique name of the task.
    fn name(&self) -> &'static str;

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

/// A task implementation that is not constrained by preconditions.
///
/// This trait is used to define tasks that should start immediately after the wiring phase, without waiting for
/// any preconditions to be met.
///
/// *Warning*. An unconstrained task may not be aware of the state of the node and is expected to catiuously check
/// any invariants it may rely on.
#[async_trait::async_trait]
pub trait UnconstrainedTask: 'static + Send {
    /// Unique name of the task.
    fn name(&self) -> &'static str;

    /// Runs the task without waiting for any precondition to be met.
    async fn run_unconstrained(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()>;
}
