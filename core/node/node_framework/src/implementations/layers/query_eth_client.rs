use anyhow::Context;
use zksync_types::{url::SensitiveUrl, L1ChainId, L2ChainId, SLChainId};
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
    l1_chain_id: L1ChainId,
    l1_rpc_url: SensitiveUrl,
    gateway_chain_id: Option<SLChainId>,
    gateway_rpc_url: Option<SensitiveUrl>,
}

impl QueryEthClientLayer {
    pub fn new(
        l1_chain_id: L1ChainId,
        l1_rpc_url: SensitiveUrl,
        gateway_chain_id: Option<SLChainId>,
        gateway_rpc_url: Option<SensitiveUrl>,
    ) -> Self {
        Self {
            l1_chain_id,
            l1_rpc_url,
            gateway_chain_id,
            gateway_rpc_url,
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
        // Both `query_client_gateway` and `query_client_l2` use the same URL, but provide different type guarantees.
        Ok(Output {
            query_client_l1: EthInterfaceResource(Box::new(
                Client::http(self.l1_rpc_url.clone())
                    .context("Client::new()")?
                    .for_network(self.l1_chain_id.into())
                    .build(),
            )),
            query_client_l2: if let Some(gateway_rpc_url) = self.gateway_rpc_url.clone() {
                let mut builder = Client::http(gateway_rpc_url).context("Client::new()")?;
                if let Some(gateway_chain_id) = self.gateway_chain_id {
                    builder =
                        builder.for_network(L2ChainId::try_from(gateway_chain_id.0).unwrap().into())
                }

                Some(L2InterfaceResource(Box::new(builder.build())))
            } else {
                None
            },
            query_client_gateway: if let Some(gateway_rpc_url) = self.gateway_rpc_url {
                let mut builder = Client::http(gateway_rpc_url).context("Client::new()")?;
                if let Some(gateway_chain_id) = self.gateway_chain_id {
                    builder = builder.for_network(gateway_chain_id.into())
                }

                Some(GatewayEthInterfaceResource(Box::new(builder.build())))
            } else {
                None
            },
        })
    }
}
