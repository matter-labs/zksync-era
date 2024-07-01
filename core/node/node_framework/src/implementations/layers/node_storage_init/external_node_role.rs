use zksync_node_storage_init::ExternalNodeRole;
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::main_node_client::MainNodeClientResource,
    resource::Unique,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

use super::NodeRoleResource;

/// Wiring layer for `MainNodeRole`.
///
/// ## Requests resources
///
/// - `MainNodeClientResource`
///
/// ## Adds resources
///
/// - `NodeRoleResource`
#[derive(Debug)]
pub struct ExternalNodeRoleLayer {
    pub l2_chain_id: L2ChainId,
}

#[async_trait::async_trait]
impl WiringLayer for ExternalNodeRoleLayer {
    fn layer_name(&self) -> &'static str {
        "main_node_role_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let MainNodeClientResource(client) = context.get_resource().await?;
        let node_role = ExternalNodeRole {
            l2_chain_id: self.l2_chain_id,
            client,
        };
        context.insert_resource(NodeRoleResource(Unique::new(Box::new(node_role))))?;
        Ok(())
    }
}
