use futures::future::BoxFuture;
use zksync_health_check::CheckHealth;

use crate::node::NodeContext;

pub mod healtcheck_server;
pub mod metadata_calculator;
pub mod prometheus_exporter;

pub trait IntoZkSyncTask: 'static + Send + Sync {
    type Config: 'static + Send + Sync;

    /// Creates a new task.
    /// Normally, at this step the task is only expected to gather required resources from `ZkSyncNode`.
    ///
    /// If additional preparations are required, they should be done in `before_launch`.
    fn create(
        node: &NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError>;
}

/// A task represents some code that "runs".
/// During its creation, it uses its own config and resources added to the `ZkSyncNode`.
///
/// TODO more elaborate docs
#[async_trait::async_trait]
pub trait ZkSyncTask: 'static + Send + Sync {
    /// Gets the healthcheck for the task, if it exists.
    /// Guaranteed to be called only once per task.
    fn healtcheck(&mut self) -> Option<Box<dyn CheckHealth>>;

    /// Runs the task.
    ///
    /// Once any of the task returns, the node will shutdown.
    /// If the task returns an error, the node will spawn an error-level log message and will return a non-zero exit code.
    ///
    /// Each task is expected to perform the required cleanup after receiving the stop signal (e.g. make sure that spawned sub-tasks are awaited).
    async fn run(self: Box<Self>) -> anyhow::Result<()>;

    /// Asynchronous hook that will be called after *each task* has finished their cleanup.
    /// It is guaranteed that no other task is running at this point, e.g. `ZkSyncNode` will invoke
    /// this hook sequentially for each task.
    ///
    /// This hook can be used to perform some cleanup that assumes exclusive access to the resources, e.g.
    /// to rollback some state.
    ///
    /// *Note*: This hook **should not** be used to perform trivial task cleanup, e.g. to wait for the spawned server to stop.
    /// By the time this hook is invoked, every component of the node is expected to stop. Not following this rule may cause
    /// the tasks that properly implement this hook to malfunction.
    fn after_node_shutdown(&self) -> Option<BoxFuture<'static, ()>> {
        None
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TaskInitError {
    #[error("Resource {0} is not provided")]
    ResourceLacking(&'static str),
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}
