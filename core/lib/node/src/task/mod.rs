//! Tasks define the "runnable" concept of the node, e.g. something that can be launched and runs until the node
//! is stopped.
//!
//! Task is normally defined by two types, one implementing two traits [`IntoZkSyncTask`], which acts like a
//! constructor, and another one, which implements [`ZkSyncTask`], providing an interface for `ZkSyncNode` to
//! implement the task lifecycle.

use futures::future::BoxFuture;

use crate::{
    node::{NodeContext, StopReceiver},
    resource::ResourceId,
};

/// Factory that can create a task.
#[async_trait::async_trait]
pub trait IntoZkSyncTask: 'static + Send + Sync {
    /// Unique name of the task.
    fn task_name(&self) -> &'static str;

    /// Creates a new task.
    ///
    /// `NodeContext` provides an interface to the utilities that task may need, e.g. ability to get resources
    /// or spawn additional tasks.
    async fn create(
        self: Box<Self>,
        node: NodeContext<'_>,
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
    /// Each task is expected to perform the required cleanup after receiving the stop signal.
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
    ResourceLacking(ResourceId),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
