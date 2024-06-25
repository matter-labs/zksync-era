use anyhow::Context;
use zksync_types::{url::SensitiveUrl, L1ChainId};
use zksync_web3_decl::client::Client;

use crate::{
    implementations::resources::eth_interface::EthInterfaceResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for Ethereum client.
///
/// ## Adds resources
///
/// - `EthInterfaceResource`
#[derive(Debug)]
pub struct QueryEthClientLayer {
    chain_id: L1ChainId,
    web3_url: SensitiveUrl,
}

impl QueryEthClientLayer {
    pub fn new(chain_id: L1ChainId, web3_url: SensitiveUrl) -> Self {
        Self { chain_id, web3_url }
    }
}

#[async_trait::async_trait]
impl WiringLayer for QueryEthClientLayer {
    fn layer_name(&self) -> &'static str {
        "query_eth_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let query_client = Client::http(self.web3_url.clone())
            .context("Client::new()")?
            .for_network(self.chain_id.into())
            .build();
        context.insert_resource(EthInterfaceResource(Box::new(query_client)))?;
        Ok(())
    }
}
