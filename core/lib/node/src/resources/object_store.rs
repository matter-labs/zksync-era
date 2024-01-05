use std::sync::Arc;

use zksync_object_store::ObjectStore;

use super::Resource;

pub const RESOURCE_NAME: &str = "common/object_store";

#[derive(Debug, Clone)]
pub struct ObjectStoreResource(pub Arc<dyn ObjectStore>);

impl Resource for ObjectStoreResource {}
