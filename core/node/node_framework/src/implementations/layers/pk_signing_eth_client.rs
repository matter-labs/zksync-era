use std::sync::Arc;

use anyhow::Context as _;
use zksync_config::{
    configs::{wallets, ContractsConfig},
    EthConfig,
};
use zksync_eth_client::clients::{PKSigningClient, QueryClient};
use zksync_types::L1ChainId;

use crate::{
    implementations::resources::eth_interface::{
        BoundEthInterfaceForBlobsResource, BoundEthInterfaceResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

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
        let query_client = QueryClient::new(self.eth_sender_config.web3_url.expose_str())
            .context("cannot create Ethereum client")?; // FIXME: take from context

        let signing_client = PKSigningClient::new_raw(
            private_key,
            self.contracts_config.diamond_proxy_addr,
            gas_adjuster_config.default_priority_fee_per_gas,
            self.l1_chain_id,
            query_client.clone(),
        );
        context.insert_resource(BoundEthInterfaceResource(Arc::new(signing_client)))?;

        if let Some(blob_operator) = &self.wallets.blob_operator {
            let private_key = blob_operator.private_key();
            let signing_client_for_blobs = PKSigningClient::new_raw(
                private_key,
                self.contracts_config.diamond_proxy_addr,
                gas_adjuster_config.default_priority_fee_per_gas,
                self.l1_chain_id,
                query_client,
            );
            context.insert_resource(BoundEthInterfaceForBlobsResource(Arc::new(
                signing_client_for_blobs,
            )))?;
        }

        Ok(())
    }
}
