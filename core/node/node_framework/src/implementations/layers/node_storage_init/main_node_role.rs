use std::sync::Arc;

use zksync_config::{ContractsConfig, GenesisConfig};
use zksync_node_storage_init::{main_node::MainNodeGenesis, NodeInitializationStrategy};

use super::NodeInitializationStrategyResource;
use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
        pools::{MasterPool, PoolResource},
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for main node initialization strategy.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `EthInterfaceResource`
///
/// ## Adds resources
///
/// - `NodeRoleResource`
#[derive(Debug)]
pub struct MainNodeInitStrategyLayer {
    pub genesis: GenesisConfig,
    pub contracts: ContractsConfig,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeInitStrategyLayer {
    fn layer_name(&self) -> &'static str {
        "main_node_role_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context
            .get_resource::<PoolResource<MasterPool>>()?
            .get()
            .await?;
        let EthInterfaceResource(l1_client) = context.get_resource()?;
        let genesis = Arc::new(MainNodeGenesis {
            contracts: self.contracts,
            genesis: self.genesis,
            l1_client,
            pool,
        });
        let strategy = NodeInitializationStrategy {
            genesis,
            snapshot_recovery: None,
            block_reverter: None,
        };

        context.insert_resource(NodeInitializationStrategyResource(strategy))?;
        Ok(())
    }
}
