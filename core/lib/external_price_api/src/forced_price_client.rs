use std::num::NonZeroU64;

use async_trait::async_trait;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::PriceAPIClient;

// Struct for a a forced price "client" (conversion ratio is always a configured "forced" ratio).
#[derive(Debug, Clone)]
pub struct ForcedPriceClient {
    ratio: BaseTokenAPIRatio,
}

impl ForcedPriceClient {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        let numerator = config
            .forced_numerator
            .expect("forced price client started with no forced numerator");
        let denominator = config
            .forced_denominator
            .expect("forced price client started with no forced denominator");

        Self {
            ratio: BaseTokenAPIRatio {
                numerator: NonZeroU64::new(numerator).unwrap(),
                denominator: NonZeroU64::new(denominator).unwrap(),
                ratio_timestamp: chrono::Utc::now(),
            },
        }
    }
}

#[async_trait]
impl PriceAPIClient for ForcedPriceClient {
    async fn fetch_ratio(&self, _token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        Ok(self.ratio)
    }
}
