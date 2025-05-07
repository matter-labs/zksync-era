//! Dependency injection for the consistency checker.

use zksync_dal::di::{MasterPool, PoolResource};
use zksync_eth_client::di::{
    BaseSettlementLayerContractsResource, SettlementLayerClient, SettlementModeResource,
};
use zksync_health_check::di::AppHealthCheckResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::ConsistencyChecker;

/// Wiring layer for the `ConsistencyChecker` (used by the external node).
#[derive(Debug)]
pub struct ConsistencyCheckerLayer {
    max_batches_to_recheck: u32,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub settlement_layer_client: SettlementLayerClient,
    pub settlement_mode: SettlementModeResource,
    pub sl_chain_contracts: BaseSettlementLayerContractsResource,
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub consistency_checker: ConsistencyChecker,
}

impl ConsistencyCheckerLayer {
    pub fn new(max_batches_to_recheck: u32) -> ConsistencyCheckerLayer {
        Self {
            max_batches_to_recheck,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ConsistencyCheckerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "consistency_checker_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let settlement_layer_client = input.settlement_layer_client;

        let singleton_pool = input.master_pool.get_singleton().await?;
        let consistency_checker = ConsistencyChecker::new(
            settlement_layer_client.into(),
            self.max_batches_to_recheck,
            singleton_pool,
            input.settlement_mode.0,
        )
        .await
        .map_err(WiringError::Internal)?
        .with_sl_diamond_proxy_addr(input.sl_chain_contracts.diamond_proxy_addr);

        input
            .app_health
            .0
            .insert_component(consistency_checker.health_check().clone())
            .map_err(WiringError::internal)?;

        Ok(Output {
            consistency_checker,
        })
    }
}

#[async_trait::async_trait]
impl Task for ConsistencyChecker {
    fn id(&self) -> TaskId {
        "consistency_checker".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
