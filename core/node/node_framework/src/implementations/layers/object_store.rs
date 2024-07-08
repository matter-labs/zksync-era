use zksync_config::ObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;

use crate::{
    implementations::resources::object_store::ObjectStoreResource,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for object store.
#[derive(Debug)]
pub struct ObjectStoreLayer {
    config: ObjectStoreConfig,
}

impl ObjectStoreLayer {
    pub fn new(config: ObjectStoreConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ObjectStoreLayer {
    type Input = ();
    type Output = ObjectStoreResource;

    fn layer_name(&self) -> &'static str {
        "object_store_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let object_store = ObjectStoreFactory::new(self.config).create_store().await?;
        let resource = ObjectStoreResource(object_store);
        Ok(resource)
    }
}
