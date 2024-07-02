use std::sync::Arc;

use zksync_config::{ContractsConfig, GenesisConfig};
use zksync_node_storage_init::MainNodeRole;

use crate::{
    implementations::resources::eth_interface::EthInterfaceResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

use super::NodeRoleResource;

/// Wiring layer for `MainNodeRole`.
///
/// ## Requests resources
///
/// - `EthInterfaceResource`
///
/// ## Adds resources
///
/// - `NodeRoleResource`
#[derive(Debug)]
pub struct MainNodeRoleLayer {
    pub genesis: GenesisConfig,
    pub contracts: ContractsConfig,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeRoleLayer {
    fn layer_name(&self) -> &'static str {
        "main_node_role_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let EthInterfaceResource(role) = context.get_resource().await?;
        let node_role = MainNodeRole {
            genesis: self.genesis,
            contracts: self.contracts,
            l1_client: role,
        };
        context.insert_resource(NodeRoleResource(Arc::new(node_role)))?;
        Ok(())
    }
}
