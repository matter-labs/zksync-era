use anyhow::Context;
use zksync_types::{url::SensitiveUrl, L1ChainId};
use zksync_web3_decl::client::Client;

use crate::{
    implementations::resources::eth_interface::EthInterfaceResource,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for Ethereum client.
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
    type Input = ();
    type Output = EthInterfaceResource;

    fn layer_name(&self) -> &'static str {
        "query_eth_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let query_client = Client::http(self.web3_url.clone())
            .context("Client::new()")?
            .for_network(self.chain_id.into())
            .build();
        Ok(EthInterfaceResource(Box::new(query_client)))
    }
}
