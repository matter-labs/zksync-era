use std::sync::Arc;

use zksync_object_store::ObjectStore;

use crate::resource::Resource;

/// A resource that provides [`ObjectStore`] to the service.
#[derive(Debug, Clone)]
pub struct ObjectStoreResource(pub Arc<dyn ObjectStore>);

impl Resource for ObjectStoreResource {
    fn name() -> String {
        "common/object_store".into()
    }
}
