use zksync_config::{configs::da_client::celestia::CelestiaSecrets, CelestiaConfig};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::celestia::CelestiaClient;

use crate::{
    implementations::resources::{
        da_client::DAClientResource, eth_interface::EthInterfaceResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
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

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for CelestiaWiringLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "celestia_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let eth_client = input.eth_client.0;

        let client: Box<dyn DataAvailabilityClient> =
            Box::new(CelestiaClient::new(self.config, self.secrets, eth_client).await?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
