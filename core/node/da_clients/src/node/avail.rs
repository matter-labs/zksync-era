use zksync_config::{configs::da_client::avail::AvailSecrets, AvailConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_eth_client::web3_decl::node::SettlementModeResource;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};

use crate::avail::AvailClient;

#[derive(Debug)]
pub struct AvailWiringLayer {
    config: AvailConfig,
    secrets: AvailSecrets,
}

#[derive(Debug, FromContext)]
pub struct Input {
    settlement_mode: SettlementModeResource,
}

impl AvailWiringLayer {
    pub fn new(config: AvailConfig, secrets: AvailSecrets) -> Self {
        Self { config, secrets }
    }
}

#[async_trait::async_trait]
impl WiringLayer for AvailWiringLayer {
    type Input = Input;
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "avail_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let sl_chain_id = input.settlement_mode.settlement_layer().chain_id();
        let client = AvailClient::new(self.config, self.secrets, sl_chain_id).await?;
        Ok(Box::new(client))
    }
}
