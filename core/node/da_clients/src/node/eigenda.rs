use zksync_config::{configs::da_client::eigenda::EigenDASecrets, EigenDAConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};

use crate::eigen_da::EigenDAClient;

#[derive(Debug)]
pub struct EigenWiringLayer {
    config: EigenDAConfig,
    secrets: EigenDASecrets,
}

impl EigenWiringLayer {
    pub fn new(config: EigenDAConfig, secrets: EigenDASecrets) -> Self {
        Self { config, secrets }
    }
}

#[derive(Debug, FromContext)]
pub struct Input {}

#[async_trait::async_trait]
impl WiringLayer for EigenWiringLayer {
    type Input = Input;
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "eigenda_client_layer"
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        let client = EigenDAClient::new(self.config, self.secrets).await?;
        Ok(Box::new(client))
    }
}
