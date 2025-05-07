use zksync_config::{configs::wallets, GasAdjusterConfig};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use super::resources::{
    BaseGatewayContractsResource, BaseL1ContractsResource, BoundEthInterfaceForBlobsResource,
    BoundEthInterfaceForL2Resource, BoundEthInterfaceResource, EthInterfaceResource,
    SettlementLayerClient,
};
use crate::clients::PKSigningClient;

/// Wiring layer for [`PKSigningClient`].
#[derive(Debug)]
pub struct PKSigningEthClientLayer {
    gas_adjuster_config: GasAdjusterConfig,
    wallets: wallets::EthSender,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    pub gateway_client: SettlementLayerClient,
    // FIXME: provide!
    pub contracts: BaseGatewayContractsResource,
    pub l1_contracts: BaseL1ContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    pub signing_client: BoundEthInterfaceResource,
    /// Only provided if the blob operator key is provided to the layer.
    pub signing_client_for_blobs: Option<BoundEthInterfaceForBlobsResource>,
    pub signing_client_for_gateway: Option<BoundEthInterfaceForL2Resource>,
}

impl PKSigningEthClientLayer {
    pub fn new(gas_adjuster_config: GasAdjusterConfig, wallets: wallets::EthSender) -> Self {
        Self {
            gas_adjuster_config,
            wallets,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for PKSigningEthClientLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "pk_signing_eth_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let private_key = self.wallets.operator.private_key();
        let gas_adjuster_config = &self.gas_adjuster_config;
        let EthInterfaceResource(query_client) = input.eth_client;

        let signing_client = PKSigningClient::new_raw(
            private_key.clone(),
            input.l1_contracts.diamond_proxy_addr,
            gas_adjuster_config.default_priority_fee_per_gas,
            input.l1_contracts.chain_id.into(),
            query_client.clone(),
        );
        let signing_client = BoundEthInterfaceResource(Box::new(signing_client));

        let signing_client_for_blobs = self.wallets.blob_operator.map(|blob_operator| {
            let private_key = blob_operator.private_key();
            let signing_client_for_blobs = PKSigningClient::new_raw(
                private_key.clone(),
                input.l1_contracts.diamond_proxy_addr,
                gas_adjuster_config.default_priority_fee_per_gas,
                input.l1_contracts.chain_id.into(),
                query_client,
            );
            BoundEthInterfaceForBlobsResource(Box::new(signing_client_for_blobs))
        });

        let signing_client_for_gateway = match input.gateway_client {
            SettlementLayerClient::L2(gateway_client) => {
                let private_key = self.wallets.operator.private_key();
                let signing_client_for_blobs = PKSigningClient::new_raw(
                    private_key.clone(),
                    input.contracts.diamond_proxy_addr,
                    gas_adjuster_config.default_priority_fee_per_gas,
                    input.contracts.chain_id,
                    gateway_client,
                );
                Some(BoundEthInterfaceForL2Resource(Box::new(
                    signing_client_for_blobs,
                )))
            }
            SettlementLayerClient::L1(_) => None,
        };

        Ok(Output {
            signing_client,
            signing_client_for_blobs,
            signing_client_for_gateway,
        })
    }
}
