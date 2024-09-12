use zksync_config::NearConfig;
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::near::NearClient;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug)]
pub struct NearWiringLayer {
    config: NearConfig,
}

impl NearWiringLayer {
    pub fn new(config: NearConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for NearWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "near_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> = Box::new(NearClient::new(self.config).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
