use anyhow::Context;
use zksync_node_framework_derive::FromContext;
use zksync_types::{settlement::SettlementMode, url::SensitiveUrl, L2ChainId};
use zksync_web3_decl::client::Client;

use crate::{
    implementations::resources::{
        eth_interface::{GatewayEthInterfaceResourceUniversalClient, UniversalClient},
        settlement_layer::{SettlementModeResource, SlChainIdResource},
    },
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for Ethereum client.
#[derive(Debug)]
pub struct GatewayClientLayer {
    l1_rpc_url: SensitiveUrl,
    gateway_rpc_url: Option<SensitiveUrl>,
}

impl GatewayClientLayer {
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
    sl_chain_id_resource: SlChainIdResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    query_client_gateway: GatewayEthInterfaceResourceUniversalClient,
}

#[async_trait::async_trait]
impl WiringLayer for GatewayClientLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "gateway_client"
    }

    async fn wire(self, input: Self::Input) -> Result<Output, WiringError> {
        Ok(Output {
            query_client_gateway: match input.initial_settlement_mode.0 {
                SettlementMode::SettlesToL1 => {
                    let mut builder = Client::http(self.l1_rpc_url).context("Client::new()")?;
                    builder = builder.for_network(input.sl_chain_id_resource.0.into());
                    GatewayEthInterfaceResourceUniversalClient(UniversalClient::L1(Box::new(
                        builder.build(),
                    )))
                }
                SettlementMode::Gateway => {
                    let mut builder =
                        Client::http(self.gateway_rpc_url.unwrap()).context("Client::new()")?;
                    builder = builder.for_network(
                        L2ChainId::new(input.sl_chain_id_resource.0 .0)
                            .unwrap()
                            .into(),
                    );
                    GatewayEthInterfaceResourceUniversalClient(UniversalClient::L2(Box::new(
                        builder.build(),
                    )))
                }
            },
        })
    }
}
