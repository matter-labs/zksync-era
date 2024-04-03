use std::sync::Arc;

use zksync_config::{
    configs::{wallets, ContractsConfig},
    ETHConfig,
};
use zksync_eth_client::clients::PKSigningClient;
use zksync_types::L1ChainId;

use crate::{
    implementations::resources::eth_interface::BoundEthInterfaceResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct PKSigningEthClientLayer {
    eth_sender_config: ETHConfig,
    contracts_config: ContractsConfig,
    l1chain_id: L1ChainId,
    wallets: wallets::EthSender,
}

impl PKSigningEthClientLayer {
    pub fn new(
        eth_sender_config: ETHConfig,
        contracts_config: ContractsConfig,
        l1chain_id: L1ChainId,
        wallets: wallets::EthSender,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            l1chain_id,
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

        let signing_client = PKSigningClient::from_config(
            &self.eth_sender_config,
            &self.contracts_config,
            self.l1chain_id,
            private_key,
        );
        context.insert_resource(BoundEthInterfaceResource(Arc::new(signing_client)))?;
        Ok(())
    }
}
