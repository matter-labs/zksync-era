use std::sync::Arc;

use tokio::sync::Mutex;
use zksync_external_price_api::{NoOpPriceAPIClient, PriceAPIClient};

use crate::resource::Resource;

/// A resource that provides [`PriceAPIClient`] implementation to the service.
#[derive(Debug, Clone)]
pub struct PriceAPIClientResource(pub Arc<Mutex<dyn PriceAPIClient>>);

impl Default for PriceAPIClientResource {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(NoOpPriceAPIClient)))
    }
}

impl Resource for PriceAPIClientResource {
    fn name() -> String {
        "common/price_api_client".into()
    }
}

impl<T: PriceAPIClient> From<Arc<Mutex<T>>> for PriceAPIClientResource {
    fn from(provider: Arc<Mutex<T>>) -> Self {
        Self(provider)
    }
}
