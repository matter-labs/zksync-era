use anyhow::Context;
use zksync_node_framework_derive::FromContext;
use zksync_types::{settlement::SettlementLayer, url::SensitiveUrl, L1ChainId, L2ChainId};
use zksync_web3_decl::client::Client;

use crate::{
    implementations::resources::{
        eth_interface::{SettlementLayerClient, SettlementLayerClientResource},
        settlement_layer::SettlementModeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for Ethereum client.
#[derive(Debug)]
pub struct SettlementLayerClientLayer {
    l1_rpc_url: SensitiveUrl,
    gateway_rpc_url: Option<SensitiveUrl>,
}

impl SettlementLayerClientLayer {
    pub fn new(l1_rpc_url: SensitiveUrl, gateway_rpc_url: Option<SensitiveUrl>) -> Self {
        Self {
            l1_rpc_url,
            gateway_rpc_url,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    initial_settlement_mode: SettlementModeResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    settlement_layer_client: SettlementLayerClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerClientLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "settlement_layer_client"
    }

    async fn wire(self, input: Self::Input) -> Result<Output, WiringError> {
        Ok(Output {
            settlement_layer_client: match input.initial_settlement_mode.settlement_layer() {
                SettlementLayer::L1(chain_id) => {
                    let mut builder = Client::http(self.l1_rpc_url).context("Client::new()")?;
                    builder = builder.for_network(L1ChainId(chain_id.0).into());
                    SettlementLayerClientResource(SettlementLayerClient::L1(Box::new(
                        builder.build(),
                    )))
                }
                SettlementLayer::Gateway(chain_id) => {
                    let mut builder =
                        Client::http(self.gateway_rpc_url.unwrap()).context("Client::new()")?;
                    builder = builder.for_network(L2ChainId::new(chain_id.0).unwrap().into());
                    SettlementLayerClientResource(SettlementLayerClient::L2(Box::new(
                        builder.build(),
                    )))
                }
            },
        })
    }
}
