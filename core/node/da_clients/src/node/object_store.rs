use zksync_config::ObjectStoreConfig;
use zksync_da_client::{node::DAClientResource, DataAvailabilityClient};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

use crate::object_store::ObjectStoreDAClient;

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
