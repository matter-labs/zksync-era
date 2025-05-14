use std::sync::Arc;

use zksync_external_price_api::{NoOpPriceAPIClient, PriceAPIClient};
use zksync_node_framework::resource::Resource;

/// A resource that provides [`PriceAPIClient`] implementation to the service.
#[derive(Debug, Clone)]
pub struct PriceAPIClientResource(pub Arc<dyn PriceAPIClient>);

impl Default for PriceAPIClientResource {
    fn default() -> Self {
        Self(Arc::new(NoOpPriceAPIClient))
    }
}

impl Resource for PriceAPIClientResource {
    fn name() -> String {
        "common/price_api_client".into()
    }
}

impl<T: PriceAPIClient> From<Arc<T>> for PriceAPIClientResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}
