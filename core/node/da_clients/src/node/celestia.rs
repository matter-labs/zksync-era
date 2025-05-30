use zksync_config::{configs::da_client::celestia::CelestiaSecrets, CelestiaConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::wiring_layer::{WiringError, WiringLayer};

use crate::celestia::CelestiaClient;

#[derive(Debug)]
pub struct CelestiaWiringLayer {
    config: CelestiaConfig,
    secrets: CelestiaSecrets,
}

impl CelestiaWiringLayer {
    pub fn new(config: CelestiaConfig, secrets: CelestiaSecrets) -> Self {
        Self { config, secrets }
    }
}

#[async_trait::async_trait]
impl WiringLayer for CelestiaWiringLayer {
    type Input = ();
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "celestia_client_layer"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        let client = CelestiaClient::new(self.config, self.secrets).await?;
        Ok(Box::new(client))
    }
}
