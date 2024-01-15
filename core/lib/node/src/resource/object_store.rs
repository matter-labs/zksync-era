use std::sync::Arc;

use zksync_object_store::ObjectStore;

use super::Resource;

#[derive(Debug, Clone)]
pub struct ObjectStoreResource(pub Arc<dyn ObjectStore>);

impl ObjectStoreResource {
    pub const RESOURCE_NAME: &str = "common/object_store";
}

impl Resource for ObjectStoreResource {}
