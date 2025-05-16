use zksync_config::ObjectStoreConfig;
use zksync_da_client::DataAvailabilityClient;
use zksync_node_framework::wiring_layer::{WiringError, WiringLayer};

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

#[async_trait::async_trait]
impl WiringLayer for ObjectStorageClientWiringLayer {
    type Input = ();
    type Output = Box<dyn DataAvailabilityClient>;

    fn layer_name(&self) -> &'static str {
        "object_store_da_layer"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        let client = ObjectStoreDAClient::new(self.config).await?;
        Ok(Box::new(client))
    }
}
