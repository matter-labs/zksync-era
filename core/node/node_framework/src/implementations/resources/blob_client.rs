use std::sync::Arc;

use zksync_l1_recovery::BlobClient;

use crate::resource::Resource;

/// A resource that provides [`BlobClient`] implementation to the service.
#[derive(Debug, Clone)]
pub struct BlobClientResource(pub Arc<dyn BlobClient>);

impl Resource for BlobClientResource {
    fn name() -> String {
        "common/blob_client".into()
    }
}

impl<T: BlobClient> From<Arc<T>> for BlobClientResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}
