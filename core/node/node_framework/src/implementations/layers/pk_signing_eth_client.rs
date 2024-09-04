use anyhow::Context as _;
use zksync_config::{
    configs::{wallets, ContractsConfig},
    EthConfig,
};
use zksync_eth_client::clients::PKSigningClient;
use zksync_types::SLChainId;

use crate::{
    implementations::resources::eth_interface::{
        BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource,
        BoundEthInterfaceResource, EthInterfaceResource, GatewayEthInterfaceResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`PKSigningClient`].
#[derive(Debug)]
pub struct PKSigningEthClientLayer {
    eth_sender_config: EthConfig,
    contracts_config: ContractsConfig,
    gateway_contracts_config: Option<ContractsConfig>,
    sl_chain_id: SLChainId,
    wallets: wallets::EthSender,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    pub gateway_client: Option<GatewayEthInterfaceResource>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub signing_client: BoundEthInterfaceResource,
    /// Only provided if the blob operator key is provided to the layer.
    pub signing_client_for_blobs: Option<BoundEthInterfaceForBlobsResource>,
    pub signing_client_for_gateway: Option<BoundEthInterfaceForL2Resource>,
}

impl PKSigningEthClientLayer {
    pub fn new(
        eth_sender_config: EthConfig,
        contracts_config: ContractsConfig,
        gateway_contracts_config: Option<ContractsConfig>,
        sl_chain_id: SLChainId,
        wallets: wallets::EthSender,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            gateway_contracts_config,
            sl_chain_id,
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
        let gas_adjuster_config = self
            .eth_sender_config
            .gas_adjuster
            .as_ref()
            .context("gas_adjuster config is missing")?;
        let EthInterfaceResource(query_client) = input.eth_client;

        let signing_client = PKSigningClient::new_raw(
            private_key.clone(),
            self.contracts_config.diamond_proxy_addr,
            gas_adjuster_config.default_priority_fee_per_gas,
            self.sl_chain_id,
            query_client.clone(),
        );
        let signing_client = BoundEthInterfaceResource(Box::new(signing_client));

        let signing_client_for_blobs = self.wallets.blob_operator.map(|blob_operator| {
            let private_key = blob_operator.private_key();
            let signing_client_for_blobs = PKSigningClient::new_raw(
                private_key.clone(),
                self.contracts_config.diamond_proxy_addr,
                gas_adjuster_config.default_priority_fee_per_gas,
                self.sl_chain_id,
                query_client,
            );
            BoundEthInterfaceForBlobsResource(Box::new(signing_client_for_blobs))
        });
        let signing_client_for_gateway = self.wallets.gateway.map(|blob_operator| {
            let private_key = blob_operator.private_key();
            let GatewayEthInterfaceResource(gateway_client) = input.gateway_client.unwrap();
            let signing_client_for_blobs = PKSigningClient::new_raw(
                private_key.clone(),
                self.gateway_contracts_config.unwrap().diamond_proxy_addr,
                gas_adjuster_config.default_priority_fee_per_gas,
                self.sl_chain_id,
                gateway_client,
            );
            BoundEthInterfaceForL2Resource(Box::new(signing_client_for_blobs))
        });

        Ok(Output {
            signing_client,
            signing_client_for_blobs,
            signing_client_for_gateway,
        })
    }
}
