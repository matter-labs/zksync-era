use std::sync::Arc;

use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_external_price_api::forced_price_client::ForcedPriceClient;

use crate::{
    implementations::resources::price_api_client::PriceAPIClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for `ForcedPriceClient`
///
/// Inserts a resource with a forced configured price to be used by the `BaseTokenRatioPersister`.
#[derive(Debug)]
pub struct ForcedPriceClientLayer {
    config: ExternalPriceApiClientConfig,
}

impl ForcedPriceClientLayer {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        Self { config }
    }

    /// Identifier of used client type.
    /// Can be used to choose the layer for the client based on configuration variables.
    pub const CLIENT_NAME: &'static str = "forced";
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub price_api_client: PriceAPIClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for ForcedPriceClientLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "forced_price_client"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let forced_client = Arc::new(ForcedPriceClient::new(self.config));

        Ok(Output {
            price_api_client: forced_client.into(),
        })
    }
}
