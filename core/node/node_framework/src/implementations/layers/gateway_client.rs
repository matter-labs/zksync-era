use anyhow::Context;
use zksync_node_framework_derive::FromContext;
use zksync_types::settlement::SettlementMode;
use zksync_types::{url::SensitiveUrl, L1ChainId, L2ChainId, SLChainId};
use zksync_web3_decl::client::Client;

use crate::implementations::resources::contracts::ContractsResource;
use crate::implementations::resources::settlement_layer::SettlementModeResource;
use crate::{
    implementations::resources::eth_interface::{
        EthInterfaceResource, GatewayEthInterfaceResource, L2InterfaceResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for Ethereum client.
#[derive(Debug)]
pub struct GatewayClientLayer {
    l1_chain_id: L1ChainId,
    l1_rpc_url: SensitiveUrl,
    gateway_rpc_url: Option<SensitiveUrl>,
}

impl GatewayClientLayer {
    pub fn new(
        l1_chain_id: L1ChainId,
        l1_rpc_url: SensitiveUrl,
        gateway_rpc_url: Option<SensitiveUrl>,
    ) -> Self {
        Self {
            l1_chain_id,
            l1_rpc_url,
            gateway_rpc_url,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    contracts: ContractsResource,
    initial_settlement_mode: SettlementModeResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    query_client_gateway: GatewayEthInterfaceResource,
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
                    builder = builder.for_network(self.l1_chain_id.into());
                    GatewayEthInterfaceResource(Box::new(builder.build()))
                }
                SettlementMode::Gateway => {
                    let mut builder =
                        Client::http(self.gateway_rpc_url.unwrap()).context("Client::new()")?;
                    builder =
                        builder.for_network(input.contracts.0.gateway_chain_id().unwrap().into());
                    GatewayEthInterfaceResource(Box::new(builder.build()))
                }
            },
        })
    }
}
