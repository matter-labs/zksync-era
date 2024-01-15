use crate::{
    node::NodeContext,
    task::{TaskInitError, ZkSyncTask},
};

// Public re-exports from external crate to minimize the required dependencies.
pub use zksync_health_check::{CheckHealth, ReactiveHealthCheck};

/// Constructor for the healtcheck task.
/// Generally equivalent to `IntoZkSyncTask`, but also accepts the list of healthcecks as an argument.
pub trait IntoHealthCheckTask: 'static + Send + Sync {
    type Config: 'static + Send + Sync;

    fn create(
        node: &NodeContext<'_>,
        healthchecks: Vec<Box<dyn CheckHealth>>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError>;
}
