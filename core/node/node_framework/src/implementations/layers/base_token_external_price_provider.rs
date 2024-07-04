use zksync_base_token_adjuster::BaseTokenRatioPersister;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::{ServiceContext, StopReceiver},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `BaseTokenRatioPersister`
///
/// Responsible for
///
/// ## Adds resources
///
#[derive(Debug)]
pub struct BaseTokenExternalPriceProviderLayer;

#[async_trait::async_trait]
impl WiringLayer for BaseTokenExternalPriceProviderLayer {
    fn layer_name(&self) -> &'static str {
        "base_token_external_price_provider"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let cg_client = CoinGeckoPriceAPIClient;

        context.insert_resource(cg_client)?;

        Ok(())
    }
}
