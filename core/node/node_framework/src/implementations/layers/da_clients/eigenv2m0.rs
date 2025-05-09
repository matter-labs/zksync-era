use zksync_config::{configs::da_client::eigenv2m0::EigenSecretsV2M0, EigenConfigV2M0};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::eigenv2m0::EigenDAClientV2M0;
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug)]
pub struct EigenV2M0WiringLayer {
    config: EigenConfigV2M0,
    secrets: EigenSecretsV2M0,
}

impl EigenV2M0WiringLayer {
    pub fn new(config: EigenConfigV2M0, secrets: EigenSecretsV2M0) -> Self {
        Self { config, secrets }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for EigenV2M0WiringLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eigenv2m0_client_layer"
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(EigenDAClientV2M0::new(self.config, self.secrets).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
