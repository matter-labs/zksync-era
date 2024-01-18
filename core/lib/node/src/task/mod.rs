//! Tasks define the "runnable" concept of the node, e.g. something that can be launched and runs until the node
//! is stopped.
//!
//! Task is normally defined by implementing two traits [`IntoZkSyncTask`], which acts like a constructor, and
//! [`ZkSyncTask`], which provides an interface for `ZkSyncNode` to implement the task lifecycle.

use futures::future::BoxFuture;

use crate::node::{NodeContext, StopReceiver};

/// Factory that can create a task.
// Note: This have to be a separate trait, since `ZkSyncTask` has to be object-safe.
pub trait IntoZkSyncTask: 'static + Send + Sync {
    /// Unique name of the task.
    const NAME: &'static str;

    /// Config type for the task.
    /// It is highly recommended for tasks to have dedicated configs, not shared with other tasks and resources.
    type Config: 'static + Send + Sync;

    /// Creates a new task.
    /// `NodeContext` provides an interface to the utilities that task may need, e.g. the runtime handle to perform asynchronous
    /// calls, or access to the resources.
    fn create(
        node: NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError>;
}

/// A task implementation.
#[async_trait::async_trait]
pub trait ZkSyncTask: 'static + Send + Sync {
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
    /// Each task is expected to perform the required cleanup after receiving the stop signal (e.g. make sure that
    /// spawned sub-tasks are awaited).
    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()>;

    /// Asynchronous hook that will be called after *each task* has finished their cleanup.
    /// It is guaranteed that no other task is running at this point, e.g. `ZkSyncNode` will invoke
    /// this hook sequentially for each task.
    ///
    /// This hook can be used to perform some cleanup that assumes exclusive access to the resources, e.g.
    /// to rollback some state.
    ///
    /// *Note*: This hook **should not** be used to perform trivial task cleanup, e.g. to wait for the spawned
    /// server to stop. By the time this hook is invoked, every component of the node is expected to stop. Not
    /// following this rule may cause the tasks that properly implement this hook to malfunction.
    fn after_node_shutdown(&self) -> Option<BoxFuture<'static, ()>> {
        None
    }
}

/// An error that can occur during the task initialization.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum TaskInitError {
    #[error("Resource {0} is not provided")]
    ResourceLacking(&'static str),
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}
