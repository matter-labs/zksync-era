use zksync_config::{configs::da_client::eigenv2m1::EigenSecretsV2M1, EigenConfigV2M1};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::eigenv2m1::EigenDAClientV2M1;
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug)]
pub struct EigenV2M1WiringLayer {
    config: EigenConfigV2M1,
    secrets: EigenSecretsV2M1,
}

impl EigenV2M1WiringLayer {
    pub fn new(config: EigenConfigV2M1, secrets: EigenSecretsV2M1) -> Self {
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
impl WiringLayer for EigenV2M1WiringLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eigenv2m1_client_layer"
    }

    async fn wire(self, _: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(EigenDAClientV2M1::new(self.config, self.secrets).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
