use anyhow::Context;
use zksync_types::{url::SensitiveUrl, L1ChainId, L2ChainId};
use zksync_web3_decl::client::Client;

use crate::{
    implementations::resources::eth_interface::{EthInterfaceResource, L2InterfaceResource},
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for Ethereum client.
///
/// ## Adds resources
///
/// - `EthInterfaceResource`
/// - `L2InterfaceResource`, if the `l2_mode` is true.
#[derive(Debug)]
pub struct QueryEthClientLayer {
    chain_id: L1ChainId,
    web3_url: SensitiveUrl,
    l2_mode: bool,
}

impl QueryEthClientLayer {
    pub fn new(chain_id: L1ChainId, web3_url: SensitiveUrl, l2_mode: bool) -> Self {
        Self {
            chain_id,
            web3_url,
            l2_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for QueryEthClientLayer {
    fn layer_name(&self) -> &'static str {
        "query_eth_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Both the L1 and L2 client have the same URL, but provide different type guarantees.

        let query_client_l1 = Client::http(self.web3_url.clone())
            .context("Client::new()")?
            .for_network(self.chain_id.into())
            .build();
        context.insert_resource(EthInterfaceResource(Box::new(query_client_l1)))?;

        if self.l2_mode {
            let query_client_l2 = Client::http(self.web3_url.clone())
                .context("Client::new()")?
                .for_network(L2ChainId(self.chain_id.0).into())
                .build();
            context.insert_resource(L2InterfaceResource(Box::new(query_client_l2.clone())))?;
        }
        Ok(())
    }
}
