use zksync_config::{configs::da_client::celestia::CelestiaSecrets, CelestiaConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::celestia::CelestiaClient;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

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

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for CelestiaWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "celestia_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(CelestiaClient::new(self.config, self.secrets).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
