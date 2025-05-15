//! Dependency injection for the consistency checker.

use std::sync::Arc;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::{
    node::contracts::SettlementLayerContractsResource,
    web3_decl::node::{SettlementLayerClient, SettlementModeResource},
    EthInterface,
};
use zksync_health_check::AppHealthCheck;
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
    settlement_layer_client: SettlementLayerClient,
    settlement_mode: SettlementModeResource,
    sl_chain_contracts: SettlementLayerContractsResource,
    master_pool: PoolResource<MasterPool>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    consistency_checker: ConsistencyChecker,
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
        let settlement_layer_client: Box<dyn EthInterface> = match input.settlement_layer_client {
            SettlementLayerClient::L1(client) => Box::new(client),
            SettlementLayerClient::L2(client) => Box::new(client),
        };

        let singleton_pool = input.master_pool.get_singleton().await?;
        let consistency_checker = ConsistencyChecker::new(
            settlement_layer_client,
            self.max_batches_to_recheck,
            singleton_pool,
            input.settlement_mode.settlement_layer(),
        )
        .await
        .map_err(WiringError::Internal)?
        .with_sl_diamond_proxy_addr(
            input
                .sl_chain_contracts
                .0
                .chain_contracts_config
                .diamond_proxy_addr,
        );

        input
            .app_health
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
