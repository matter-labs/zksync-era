use zksync_config::AvailConfig;
use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::wiring_layer::{WiringError, WiringLayer};

use crate::avail::AvailClient;

#[derive(Debug)]
pub struct AvailWiringLayer {
    config: AvailConfig,
}

impl AvailWiringLayer {
    pub fn new(config: AvailConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for AvailWiringLayer {
    type Input = ();
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "avail_client_layer"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        let client = AvailClient::new(self.config).await?;
        Ok(Box::new(client))
    }
}
