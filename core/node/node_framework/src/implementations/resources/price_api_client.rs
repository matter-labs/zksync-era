use std::sync::Arc;

use zksync_external_price_api::PriceAPIClient;

use crate::resource::Resource;

/// A resource that provides [`PriceAPIClient`] implementation to the service.
#[derive(Clone)]
pub struct PriceAPIClientResource(pub Arc<dyn PriceAPIClient>);

impl Resource for PriceAPIClientResource {
    fn name() -> String {
        "common/price_api_client".into()
    }
}
