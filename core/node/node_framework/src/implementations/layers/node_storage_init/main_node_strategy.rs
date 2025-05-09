use std::sync::Arc;

use zksync_config::GenesisConfig;
use zksync_node_storage_init::{main_node::MainNodeGenesis, NodeInitializationStrategy};

use super::NodeInitializationStrategyResource;
use crate::{
    implementations::resources::{
        contracts::SettlementLayerContractsResource,
        eth_interface::EthInterfaceResource,
        pools::{MasterPool, PoolResource},
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for main node initialization strategy.
#[derive(Debug)]
pub struct MainNodeInitStrategyLayer {
    pub genesis: GenesisConfig,
    pub event_expiration_blocks: u64,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub eth_interface: EthInterfaceResource,
    pub contracts_resource: SettlementLayerContractsResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub strategy: NodeInitializationStrategyResource,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeInitStrategyLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "main_node_role_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let EthInterfaceResource(l1_client) = input.eth_interface;
        let genesis = Arc::new(MainNodeGenesis {
            contracts: input.contracts_resource.0,
            genesis: self.genesis,
            event_expiration_blocks: self.event_expiration_blocks,
            l1_client,
            pool,
        });
        let strategy = NodeInitializationStrategy {
            genesis,
            snapshot_recovery: None,
            block_reverter: None,
        };

        Ok(Output {
            strategy: strategy.into(),
        })
    }
}
