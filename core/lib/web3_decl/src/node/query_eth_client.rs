use anyhow::Context;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};
use zksync_types::{url::SensitiveUrl, L1ChainId};

use super::resources::EthInterfaceResource;
use crate::client::Client;

/// Wiring layer for Ethereum client.
#[derive(Debug)]
pub struct QueryEthClientLayer {
    l1_chain_id: L1ChainId,
    l1_rpc_url: SensitiveUrl,
}

impl QueryEthClientLayer {
    pub fn new(l1_chain_id: L1ChainId, l1_rpc_url: SensitiveUrl) -> Self {
        Self {
            l1_chain_id,
            l1_rpc_url,
        }
    }
}

#[derive(Debug, IntoContext)]
pub struct Output {
    query_client_l1: EthInterfaceResource,
}

#[async_trait::async_trait]
impl WiringLayer for QueryEthClientLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "query_eth_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Output, WiringError> {
        Ok(Output {
            query_client_l1: EthInterfaceResource(Box::new(
                Client::http(self.l1_rpc_url.clone())
                    .context("Client::new()")?
                    .for_network(self.l1_chain_id.into())
                    .build(),
            )),
        })
    }
}
