use std::sync::Arc;

use zksync_object_store::ObjectStore;

use crate::resource::Resource;

/// Wrapper for the object store.
#[derive(Debug, Clone)]
pub struct ObjectStoreResource(pub Arc<dyn ObjectStore>);

impl Resource for ObjectStoreResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/object_store".into()
    }
}
