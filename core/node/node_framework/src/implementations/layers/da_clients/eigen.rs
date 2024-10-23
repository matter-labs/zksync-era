use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::eigen::EigenClient;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

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

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for EigenWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eigen_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(EigenClient::new(self.config, self.secrets).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
