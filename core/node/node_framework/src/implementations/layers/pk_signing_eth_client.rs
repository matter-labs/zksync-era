use std::sync::Arc;

use zksync_config::{ContractsConfig, ETHClientConfig, ETHSenderConfig};
use zksync_eth_client::clients::PKSigningClient;

use crate::{
    implementations::resources::eth_interface::BoundEthInterfaceResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct PKSigningEthClientLayer {
    eth_sender_config: ETHSenderConfig,
    contracts_config: ContractsConfig,
    eth_client_config: ETHClientConfig,
}

impl PKSigningEthClientLayer {
    pub fn new(
        eth_sender_config: ETHSenderConfig,
        contracts_config: ContractsConfig,
        eth_client_config: ETHClientConfig,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            eth_client_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for PKSigningEthClientLayer {
    fn layer_name(&self) -> &'static str {
        "pk_signing_eth_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        if self.eth_sender_config.sender.private_key().is_none() {
            return Err(WiringError::Configuration(
                "Private key is missing in ETHSenderConfig".to_string(),
            ));
        }

        let signing_client = PKSigningClient::from_config(
            &self.eth_sender_config,
            &self.contracts_config,
            &self.eth_client_config,
        );
        context.insert_resource(BoundEthInterfaceResource(Arc::new(signing_client)))?;
        Ok(())
    }
}
