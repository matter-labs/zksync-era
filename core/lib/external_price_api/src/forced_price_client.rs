use std::num::NonZeroU64;

use async_trait::async_trait;
use rand::Rng;
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
    // Returns a ratio which is 10% higher or lower than the configured forced ratio.
    async fn fetch_ratio(&self, _token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        let mut rng = rand::thread_rng();

        let numerator_range = (
            (self.ratio.numerator.get() as f64 * 0.9).round() as u64,
            (self.ratio.numerator.get() as f64 * 1.1).round() as u64,
        );

        let denominator_range = (
            (self.ratio.denominator.get() as f64 * 0.9).round() as u64,
            (self.ratio.denominator.get() as f64 * 1.1).round() as u64,
        );

        let new_numerator = rng.gen_range(numerator_range.0..=numerator_range.1);
        let new_denominator = rng.gen_range(denominator_range.0..=denominator_range.1);

        let adjusted_ratio = BaseTokenAPIRatio {
            numerator: NonZeroU64::new(new_numerator).unwrap_or(self.ratio.numerator),
            denominator: NonZeroU64::new(new_denominator).unwrap_or(self.ratio.denominator),
            ratio_timestamp: chrono::Utc::now(),
        };

        Ok(adjusted_ratio)
    }
}
