use std::sync::Arc;

use zksync_config::configs::contracts::ecosystem::L1SpecificContracts;
use zksync_external_price_api::{NoOpPriceAPIClient, PriceAPIClient};
use zksync_node_framework::resource::Resource;

// FIXME: probably out of place here
#[derive(Debug, Clone)]
pub struct L1EcosystemContractsResource(pub L1SpecificContracts);

impl Resource for L1EcosystemContractsResource {
    fn name() -> String {
        "common/l1_ecosystem_contracts".into()
    }
}

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
