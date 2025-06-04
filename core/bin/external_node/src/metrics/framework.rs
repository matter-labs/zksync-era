use std::time::Duration;

use zksync_dal::{
    node::{MasterPool, PoolResource},
    ConnectionPool, Core, CoreDal as _,
};
use zksync_node_framework::{
    FromContext, IntoContext, StopReceiver, Task, TaskId, WiringError, WiringLayer,
};
use zksync_types::{L1ChainId, L2ChainId, SLChainId};

use super::EN_METRICS;

#[derive(Debug)]
pub struct ExternalNodeMetricsLayer {
    pub l1_chain_id: L1ChainId,
    pub sl_chain_id: SLChainId,
    pub l2_chain_id: L2ChainId,
    pub postgres_pool_size: u32,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub task: ProtocolVersionMetricsTask,
}

#[async_trait::async_trait]
impl WiringLayer for ExternalNodeMetricsLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "external_node_metrics"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        EN_METRICS.observe_config(
            self.l1_chain_id,
            self.sl_chain_id,
            self.l2_chain_id,
            self.postgres_pool_size,
        );

        let pool = input.master_pool.get_singleton().await?;
        let task = ProtocolVersionMetricsTask { pool };
        Ok(Output { task })
    }
}

#[derive(Debug)]
pub struct ProtocolVersionMetricsTask {
    pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl Task for ProtocolVersionMetricsTask {
    fn id(&self) -> TaskId {
        "en_protocol_version_metrics".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        const QUERY_INTERVAL: Duration = Duration::from_secs(10);

        while !*stop_receiver.0.borrow_and_update() {
            let maybe_protocol_version = self
                .pool
                .connection()
                .await?
                .protocol_versions_dal()
                .last_used_version_id()
                .await;
            if let Some(version) = maybe_protocol_version {
                EN_METRICS.protocol_version.set(version as u64);
            }

            tokio::time::timeout(QUERY_INTERVAL, stop_receiver.0.changed())
                .await
                .ok();
        }
        Ok(())
    }
}
