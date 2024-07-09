use std::sync::Arc;

use zksync_external_price_api::NoOpPriceAPIClient;

use crate::{
    implementations::resources::price_api_client::PriceAPIClientResource,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for `NoOpExternalPriceApiClient`
///
/// Inserts a resource with a no-op client to get base token prices to be used by the `BaseTokenRatioPersister`.
#[derive(Debug)]
pub struct NoOpExternalPriceApiClientLayer;

impl NoOpExternalPriceApiClientLayer {
    /// Identifier of used client type.
    /// Can be used to choose the layer for the client based on configuration variables.
    pub const CLIENT_NAME: &'static str = "no-op";
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub price_api_client: PriceAPIClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for NoOpExternalPriceApiClientLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "no_op_external_price_api_client"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let no_op_client = Arc::new(NoOpPriceAPIClient {});

        Ok(Output {
            price_api_client: no_op_client.into(),
        })
    }
}
