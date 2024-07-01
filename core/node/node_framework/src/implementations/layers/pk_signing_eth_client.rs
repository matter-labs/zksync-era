use anyhow::Context as _;
use zksync_config::{
    configs::{wallets, ContractsConfig},
    EthConfig,
};
use zksync_eth_client::clients::PKSigningClient;
use zksync_types::L1ChainId;

use crate::{
    implementations::resources::eth_interface::{
        BoundEthInterfaceForBlobsResource, BoundEthInterfaceResource, EthInterfaceResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for [`PKSigningClient`].
///
/// ## Requests resources
///
/// - `EthInterfaceResource`
///
/// ## Adds resources
///
/// - `BoundEthInterfaceResource`
/// - `BoundEthInterfaceForBlobsResource` (if key for blob operator is provided)
#[derive(Debug)]
pub struct PKSigningEthClientLayer {
    eth_sender_config: EthConfig,
    contracts_config: ContractsConfig,
    l1_chain_id: L1ChainId,
    wallets: wallets::EthSender,
}

impl PKSigningEthClientLayer {
    pub fn new(
        eth_sender_config: EthConfig,
        contracts_config: ContractsConfig,
        l1_chain_id: L1ChainId,
        wallets: wallets::EthSender,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            l1_chain_id,
            wallets,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for PKSigningEthClientLayer {
    fn layer_name(&self) -> &'static str {
        "pk_signing_eth_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let private_key = self.wallets.operator.private_key();
        let gas_adjuster_config = self
            .eth_sender_config
            .gas_adjuster
            .as_ref()
            .context("gas_adjuster config is missing")?;
        let EthInterfaceResource(query_client) = context.get_resource()?;

        let signing_client = PKSigningClient::new_raw(
            private_key.clone(),
            self.contracts_config.diamond_proxy_addr,
            gas_adjuster_config.default_priority_fee_per_gas,
            self.l1_chain_id,
            query_client.clone(),
        );
        context.insert_resource(BoundEthInterfaceResource(Box::new(signing_client)))?;

        if let Some(blob_operator) = &self.wallets.blob_operator {
            let private_key = blob_operator.private_key();
            let signing_client_for_blobs = PKSigningClient::new_raw(
                private_key.clone(),
                self.contracts_config.diamond_proxy_addr,
                gas_adjuster_config.default_priority_fee_per_gas,
                self.l1_chain_id,
                query_client,
            );
            context.insert_resource(BoundEthInterfaceForBlobsResource(Box::new(
                signing_client_for_blobs,
            )))?;
        }

        Ok(())
    }
}
