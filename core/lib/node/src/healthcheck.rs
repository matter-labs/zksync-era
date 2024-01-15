// Public re-exports from external crate to minimize the required dependencies.
pub use zksync_health_check::{CheckHealth, ReactiveHealthCheck};

use crate::{
    node::NodeContext,
    task::{TaskInitError, ZkSyncTask},
};

pub trait IntoHealthCheckTask: 'static + Send + Sync {
    type Config: 'static + Send + Sync;

    fn create(
        node: &NodeContext<'_>,
        healthchecks: Vec<Box<dyn CheckHealth>>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError>;
}
