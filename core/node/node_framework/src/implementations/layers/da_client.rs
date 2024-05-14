use anyhow::Context as _;
use zksync_config::{configs::da_dispatcher::DADispatcherConfig, EthConfig};
use zksync_eth_client::clients::PKSigningClient;

use crate::{
    implementations::resources::{
        da_interface::DAInterfaceResource,
        eth_interface::{
            BoundEthInterfaceForBlobsResource, BoundEthInterfaceResource, EthInterfaceResource,
        },
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct DataAvailabilityClientLayer {
    eth_sender_config: EthConfig,
    da_dispatcher_config: DADispatcherConfig,
}

impl DataAvailabilityClientLayer {
    pub fn new(eth_sender_config: EthConfig, da_dispatcher_config: DADispatcherConfig) -> Self {
        Self {
            eth_sender_config,
            da_dispatcher_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityClientLayer {
    fn layer_name(&self) -> &'static str {
        "data_availability_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let private_key = self.wallets.operator.private_key();
        let gas_adjuster_config = self
            .eth_sender_config
            .gas_adjuster
            .as_ref()
            .context("gas_adjuster config is missing")?;
        let EthInterfaceResource(query_client) = context.get_resource().await?;

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
            context.insert_resource(DAInterfaceResource(Box::new(signing_client_for_blobs)))?;
        }

        Ok(())
    }
}
