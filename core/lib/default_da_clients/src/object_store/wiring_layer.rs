use zksync_config::ObjectStoreConfig;
use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::{
    implementations::resources::da_client::DAClientResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

use crate::object_store::client::ObjectStoreDAClient;

#[derive(Debug)]
pub struct ObjectStorageClientWiringLayer {
    config: ObjectStoreConfig,
}

impl ObjectStorageClientWiringLayer {
    pub fn new(config: ObjectStoreConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ObjectStorageClientWiringLayer {
    fn layer_name(&self) -> &'static str {
        "object_store_da_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(ObjectStoreDAClient::new(self.config).await?);

        context.insert_resource(DAClientResource(client))?;

        Ok(())
    }
}
