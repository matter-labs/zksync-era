use zksync_config::ObjectStoreConfig;
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::object_store::ObjectStoreDAClient;

use crate::{
    implementations::resources::da_client::DAClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

#[derive(Debug)]
pub struct ObjectStorageClientWiringLayer {
    config: ObjectStoreConfig,
}

impl ObjectStorageClientWiringLayer {
    pub fn new(config: ObjectStoreConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for ObjectStorageClientWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "object_store_da_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(ObjectStoreDAClient::new(self.config).await?);

        Ok(Output {
            client: DAClientResource(client),
        })
    }
}
