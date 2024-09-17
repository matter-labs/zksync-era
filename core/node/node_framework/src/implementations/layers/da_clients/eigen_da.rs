use zksync_config::configs::da_client::eigen_da::EigenDAConfig;
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::eigen_da::EigenDAClient;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug, Default)]
pub struct EigenDAWiringLayer {
    config: EigenDAConfig,
}

impl EigenDAWiringLayer {
    pub fn new(config: EigenDAConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for EigenDAWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eigen_da_client_layer"
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(EigenDAClient::new(self.config).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
