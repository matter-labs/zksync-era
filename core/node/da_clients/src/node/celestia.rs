use zksync_config::{configs::da_client::celestia::CelestiaSecrets, CelestiaConfig};
use zksync_basic_types::L2ChainId;
use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};
use zksync_web3_decl::client::{DynClient, L1};

use crate::celestia::CelestiaClient;

#[derive(Debug)]
pub struct CelestiaWiringLayer {
    config: CelestiaConfig,
    secrets: CelestiaSecrets,
    l2_chain_id: L2ChainId
}

impl CelestiaWiringLayer {
    pub fn new(config: CelestiaConfig, secrets: CelestiaSecrets, l2_chain_id: L2ChainId) -> Self {
        Self { config, secrets, l2_chain_id }
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    eth_client: Box<DynClient<L1>>,
}

#[async_trait::async_trait]
impl WiringLayer for CelestiaWiringLayer {
    type Input = Input;
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "celestia_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let client = CelestiaClient::new(self.config, self.secrets, input.eth_client, self.l2_chain_id).await?;
        Ok(Box::new(client))
    }
}
