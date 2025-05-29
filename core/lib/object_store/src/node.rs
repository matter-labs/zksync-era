//! Dependency injection for object store.

use std::sync::Arc;

use zksync_config::ObjectStoreConfig;
use zksync_node_framework::{
    resource::{self, Resource},
    WiringError, WiringLayer,
};

use crate::{ObjectStore, ObjectStoreFactory};

impl Resource<resource::Shared> for dyn ObjectStore {
    fn name() -> String {
        "common/object_store".into()
    }
}

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
    type Output = Arc<dyn ObjectStore>;

    fn layer_name(&self) -> &'static str {
        "object_store_layer"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        let object_store = ObjectStoreFactory::new(self.config).create_store().await?;
        Ok(object_store)
    }
}
