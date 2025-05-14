use std::sync::Arc;

use zksync_config::GenesisConfig;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_di::contracts::SettlementLayerContractsResource;
use zksync_web3_decl::node::EthInterfaceResource;

use super::NodeInitializationStrategyResource;
use crate::{main_node::MainNodeGenesis, NodeInitializationStrategy};

/// Wiring layer for main node initialization strategy.
#[derive(Debug)]
pub struct MainNodeInitStrategyLayer {
    pub genesis: GenesisConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub eth_interface: EthInterfaceResource,
    pub contracts: SettlementLayerContractsResource,
}

#[derive(Debug, IntoContext)]
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
            contracts: input.contracts.0,
            genesis: self.genesis,
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
