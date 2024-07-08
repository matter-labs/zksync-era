use std::sync::Arc;

use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_external_price_api::coingecko_api::CoinGeckoPriceAPIClient;

use crate::{
    implementations::resources::price_api_client::PriceAPIClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for `ExternalPriceApiClient`
///
/// Responsible for inserting all the resources to get base token prices from external price feeds to
/// be used by the `BaseTokenRatioPersister`.
#[derive(Debug)]
pub struct ExternalPriceApiClientsLayer {
    config: ExternalPriceApiClientConfig,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub price_api_client: PriceAPIClientResource,
}

impl ExternalPriceApiClientsLayer {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ExternalPriceApiClientsLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "external_price_api_clients"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let cg_client = Arc::new(CoinGeckoPriceAPIClient::new(self.config));

        Ok(Output {
            price_api_client: cg_client.into(),
        })
    }
}
