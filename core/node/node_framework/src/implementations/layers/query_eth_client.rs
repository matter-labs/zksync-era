use anyhow::Context;
use zksync_types::{url::SensitiveUrl, L2ChainId, SLChainId};
use zksync_web3_decl::client::Client;

use crate::{
    implementations::resources::eth_interface::{
        EthInterfaceResource, GatewayEthInterfaceResource, L2InterfaceResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for Ethereum client.
#[derive(Debug)]
pub struct QueryEthClientLayer {
    chain_id: SLChainId,
    web3_url: SensitiveUrl,
    gateway_web3_url: Option<SensitiveUrl>,
}

impl QueryEthClientLayer {
    pub fn new(
        chain_id: SLChainId,
        web3_url: SensitiveUrl,
        gateway_web3_url: Option<SensitiveUrl>,
    ) -> Self {
        Self {
            chain_id,
            web3_url,
            gateway_web3_url,
        }
    }
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    query_client_l1: EthInterfaceResource,
    query_client_l2: Option<L2InterfaceResource>,
    query_client_gateway: Option<GatewayEthInterfaceResource>,
}

#[async_trait::async_trait]
impl WiringLayer for QueryEthClientLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "query_eth_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Output, WiringError> {
        // Both the L1 and L2 client have the same URL, but provide different type guarantees.
        Ok(Output {
            query_client_l1: EthInterfaceResource(Box::new(
                Client::http(self.web3_url.clone())
                    .context("Client::new()")?
                    .for_network(self.chain_id.into())
                    .build(),
            )),
            query_client_l2: if self.gateway_web3_url.is_some() {
                Some(L2InterfaceResource(Box::new(
                    Client::http(
                        self.gateway_web3_url
                            .clone()
                            .expect("gateway url is required"),
                    )
                    .context("Client::new()")?
                    .for_network(L2ChainId::try_from(self.chain_id.0).unwrap().into())
                    .build(),
                )))
            } else {
                None
            },
            query_client_gateway: if self.gateway_web3_url.is_some() {
                Some(GatewayEthInterfaceResource(Box::new(
                    Client::http(
                        self.gateway_web3_url
                            .clone()
                            .expect("gateway url is required"),
                    )
                    .context("Client::new()")?
                    .build(),
                )))
            } else {
                None
            },
        })
    }
}
