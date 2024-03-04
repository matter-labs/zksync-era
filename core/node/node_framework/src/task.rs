//! Tasks define the "runnable" concept of the node, e.g. something that can be launched and runs until the node
//! is stopped.

use crate::service::StopReceiver;

/// A task implementation.
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
