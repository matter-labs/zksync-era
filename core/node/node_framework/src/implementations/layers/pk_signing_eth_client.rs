use anyhow::Context as _;
use zksync_config::{
    configs::{gateway::GatewayChainConfig, wallets, ContractsConfig},
    EthConfig,
};
use zksync_eth_client::{clients::PKSigningClient, EthInterface};

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
    gateway_chain_config: Option<GatewayChainConfig>,
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
        gateway_chain_config: Option<GatewayChainConfig>,
        wallets: wallets::EthSender,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            gateway_chain_config,
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

        let l1_chain_id = query_client
            .fetch_chain_id()
            .await
            .map_err(WiringError::internal)?;
        let signing_client = PKSigningClient::new_raw(
            private_key.clone(),
            self.contracts_config.diamond_proxy_addr,
            gas_adjuster_config.default_priority_fee_per_gas,
            l1_chain_id,
            query_client.clone(),
        );
        let signing_client = BoundEthInterfaceResource(Box::new(signing_client));

        let signing_client_for_blobs = self.wallets.blob_operator.map(|blob_operator| {
            let private_key = blob_operator.private_key();
            let signing_client_for_blobs = PKSigningClient::new_raw(
                private_key.clone(),
                self.contracts_config.diamond_proxy_addr,
                gas_adjuster_config.default_priority_fee_per_gas,
                l1_chain_id,
                query_client,
            );
            BoundEthInterfaceForBlobsResource(Box::new(signing_client_for_blobs))
        });

        let signing_client_for_gateway = if let (Some(client), Some(gateway_contracts)) =
            (&input.gateway_client, self.gateway_chain_config.as_ref())
        {
            if gateway_contracts.gateway_chain_id.0 != 0u64 {
                let private_key = self.wallets.operator.private_key();
                let GatewayEthInterfaceResource(gateway_client) = client;
                let signing_client_for_blobs = PKSigningClient::new_raw(
                    private_key.clone(),
                    gateway_contracts.diamond_proxy_addr,
                    gas_adjuster_config.default_priority_fee_per_gas,
                    gateway_contracts.gateway_chain_id,
                    gateway_client.clone(),
                );
                Some(BoundEthInterfaceForL2Resource(Box::new(
                    signing_client_for_blobs,
                )))
            } else {
                None
            }
        } else {
            None
        };

        Ok(Output {
            signing_client,
            signing_client_for_blobs,
            signing_client_for_gateway,
        })
    }
}
