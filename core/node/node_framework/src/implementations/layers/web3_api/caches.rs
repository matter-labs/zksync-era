use std::time::Duration;

use zksync_node_api_server::web3::mempool_cache::{MempoolCache, MempoolCacheUpdateTask};
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::{
        pools::{PoolResource, ReplicaPool},
        web3_api::MempoolCacheResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for API mempool cache.
#[derive(Debug)]
pub struct MempoolCacheLayer {
    capacity: usize,
    update_interval: Duration,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub replica_pool: PoolResource<ReplicaPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub mempool_cache: MempoolCacheResource,
    #[context(task)]
    pub update_task: MempoolCacheUpdateTask,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "mempool_cache_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let replica_pool = input.replica_pool.get().await?;
        let mempool_cache = MempoolCache::new(self.capacity);
        let update_task = mempool_cache.update_task(replica_pool, self.update_interval);
        Ok(Output {
            mempool_cache: mempool_cache.into(),
            update_task,
        })
    }
}

#[async_trait::async_trait]
impl Task for MempoolCacheUpdateTask {
    fn id(&self) -> TaskId {
        "mempool_cache_update_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
