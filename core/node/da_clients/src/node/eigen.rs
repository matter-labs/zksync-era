use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::wiring_layer::{WiringError, WiringLayer};

use crate::eigen::EigenDAClient;

#[derive(Debug)]
pub struct EigenWiringLayer {
    config: EigenConfig,
    secrets: EigenSecrets,
}

impl EigenWiringLayer {
    pub fn new(config: EigenConfig, secrets: EigenSecrets) -> Self {
        Self { config, secrets }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EigenWiringLayer {
    type Input = ();
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "eigen_client_layer"
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        let client = EigenDAClient::new(self.config, self.secrets).await?;
        Ok(Box::new(client))
    }
}
