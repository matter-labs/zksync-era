use zksync_config::{configs::da_client::avail::AvailSecrets, AvailConfig};
use zksync_da_client::{node::DAClientResource, DataAvailabilityClient};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

use crate::avail::AvailClient;

#[derive(Debug)]
pub struct AvailWiringLayer {
    config: AvailConfig,
    secrets: AvailSecrets,
}

impl AvailWiringLayer {
    pub fn new(config: AvailConfig, secrets: AvailSecrets) -> Self {
        Self { config, secrets }
    }
}

#[derive(Debug, IntoContext)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for AvailWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "avail_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(AvailClient::new(self.config, self.secrets).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
