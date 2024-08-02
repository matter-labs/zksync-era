use anyhow::Context;
use zksync_types::{settlement::SettlementMode, url::SensitiveUrl, L2ChainId, SLChainId};
use zksync_web3_decl::client::{Client, L1, L2};

use crate::{
    implementations::resources::eth_interface::{EthInterfaceResource, L2InterfaceResource},
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for Ethereum client.
#[derive(Debug)]
pub struct QueryEthClientLayer {
    chain_id: SLChainId,
    web3_url: SensitiveUrl,
    settlement_mode: SettlementMode,
}

impl QueryEthClientLayer {
    pub fn new(
        chain_id: SLChainId,
        web3_url: SensitiveUrl,
        settlement_mode: SettlementMode,
    ) -> Self {
        Self {
            chain_id,
            web3_url,
            settlement_mode,
        }
    }
}

#[derive(Debug)]
pub struct Output {
    query_client_l1: Client<L1>,
    query_client_l2: Option<Client<L2>>,
}

impl IntoContext for Output {
    fn into_context(self, context: &mut ServiceContext<'_>) -> Result<(), WiringError> {
        context.insert_resource(EthInterfaceResource(Box::new(self.query_client_l1)))?;
        if let Some(query_client_l2) = self.query_client_l2 {
            context.insert_resource(L2InterfaceResource(Box::new(query_client_l2)))?;
        }
        Ok(())
    }
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
            query_client_l1: Client::http(self.web3_url.clone())
                .context("Client::new()")?
                .for_network(self.chain_id.into())
                .build(),
            query_client_l2: if self.settlement_mode.is_gateway() {
                Some(
                    Client::http(self.web3_url.clone())
                        .context("Client::new()")?
                        .for_network(L2ChainId::try_from(self.chain_id.0).unwrap().into())
                        .build(),
                )
            } else {
                None
            },
        })
    }
}
