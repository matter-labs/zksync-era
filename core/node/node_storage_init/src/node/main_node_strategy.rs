use std::sync::Arc;

use zksync_config::GenesisConfig;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};
use zksync_shared_resources::contracts::SettlementLayerContractsResource;
use zksync_web3_decl::client::{DynClient, L1};

use crate::{main_node::MainNodeGenesis, NodeInitializationStrategy};

/// Wiring layer for main node initialization strategy.
#[derive(Debug)]
pub struct MainNodeInitStrategyLayer {
    pub genesis: GenesisConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    l1_client: Box<DynClient<L1>>,
    contracts: SettlementLayerContractsResource,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeInitStrategyLayer {
    type Input = Input;
    type Output = NodeInitializationStrategy;

    fn layer_name(&self) -> &'static str {
        "main_node_role_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let genesis = Arc::new(MainNodeGenesis {
            contracts: input.contracts.0,
            genesis: self.genesis,
            l1_client: input.l1_client,
            pool,
        });

        Ok(NodeInitializationStrategy {
            genesis,
            snapshot_recovery: None,
            block_reverter: None,
        })
    }
}
