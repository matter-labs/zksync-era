use std::sync::Arc;

use zksync_config::configs::BaseTokenApiClientConfig;
use zksync_external_price_api::coingecko_api::CoinGeckoPriceAPIClient;

use crate::{
    implementations::resources::price_api_client::PriceAPIClientResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `BaseTokenRatioPersister`
///
/// Responsible for
///
/// ## Adds resources
///
/// - PriceAPIClientResource
#[derive(Debug)]
pub struct BaseTokenExternalPriceApiClient {
    config: BaseTokenApiClientConfig,
}

impl BaseTokenExternalPriceApiClient {
    pub fn new(config: BaseTokenApiClientConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BaseTokenExternalPriceApiClient {
    fn layer_name(&self) -> &'static str {
        "base_token_external_price_provider"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let cg_client = CoinGeckoPriceAPIClient::new(self.config);

        context.insert_resource(PriceAPIClientResource(Arc::new(cg_client)))?;

        Ok(())
    }
}
