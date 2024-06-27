use zksync_config::ObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;

use crate::{
    implementations::resources::object_store::ObjectStoreResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for object store.
///
/// ## Adds resources
///
/// - `ObjectStoreResource`
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
    fn layer_name(&self) -> &'static str {
        "object_store_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let object_store = ObjectStoreFactory::new(self.config).create_store().await?;
        context.insert_resource(ObjectStoreResource(object_store))?;
        Ok(())
    }
}
