use std::time::Duration;

use zksync_node_api_server::web3::mempool_cache::{self, MempoolCache};

use crate::{
    implementations::resources::{
        pools::{PoolResource, ReplicaPool},
        web3_api::MempoolCacheResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for API mempool cache.
///
/// ## Requests resources
///
/// - `PoolResource<ReplicaPool>`
///
/// ## Adds resources
///
/// - `MempoolCacheResource`
///
/// ## Adds tasks
///
/// - `MempoolCacheUpdateTask`
#[derive(Debug)]
pub struct MempoolCacheLayer {
    capacity: usize,
    update_interval: Duration,
}

impl MempoolCacheLayer {
    pub fn new(capacity: usize, update_interval: Duration) -> Self {
        Self {
            capacity,
            update_interval,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for MempoolCacheLayer {
    fn layer_name(&self) -> &'static str {
        "mempool_cache_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<PoolResource<ReplicaPool>>().await?;
        let replica_pool = pool_resource.get().await?;
        let mempool_cache = MempoolCache::new(self.capacity);
        let update_task = mempool_cache.update_task(replica_pool, self.update_interval);
        context.add_task(Box::new(MempoolCacheUpdateTask(update_task)));
        context.insert_resource(MempoolCacheResource(mempool_cache))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct MempoolCacheUpdateTask(mempool_cache::MempoolCacheUpdateTask);

#[async_trait::async_trait]
impl Task for MempoolCacheUpdateTask {
    fn id(&self) -> TaskId {
        "mempool_cache_update_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0).await
    }
}
