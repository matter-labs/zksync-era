use std::{
    cmp::{max, min},
    num::NonZeroU64,
};

use async_trait::async_trait;
use rand::Rng;
use tokio::sync::RwLock;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::PriceAPIClient;

const VARIATION_RANGE: f64 = 0.2; //20%
const NEXT_VALUE_VARIATION_RANGE: f64 = 0.03; //3%

// Struct for a forced price "client" (conversion ratio is always a configured "forced" ratio).
#[derive(Debug)]
pub struct ForcedPriceClient {
    ratio: BaseTokenAPIRatio,
    previousNumerator: RwLock<NonZeroU64>,
    fluctuation: Option<u32>,
}

impl ForcedPriceClient {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        let forced_price_client_config = config
            .forced
            .expect("forced price client started with no config");

        let numerator = forced_price_client_config
            .numerator
            .expect("forced price client started with no forced numerator");
        let denominator = forced_price_client_config
            .denominator
            .expect("forced price client started with no forced denominator");
        let fluctuation = forced_price_client_config
            .fluctuation
            .map(|x| x.clamp(0, 100));

        Self {
            ratio: BaseTokenAPIRatio {
                numerator: NonZeroU64::new(numerator).unwrap(),
                denominator: NonZeroU64::new(denominator).unwrap(),
                ratio_timestamp: chrono::Utc::now(),
            },
            previousNumerator: RwLock::new(NonZeroU64::new(numerator).unwrap()),
            fluctuation,
        }
    }
}

#[async_trait]
impl PriceAPIClient for ForcedPriceClient {
    /// Returns a ratio which is 10% higher or lower than the configured forced ratio,
    /// but not different more than 3% than the last value
    async fn fetch_ratio(&self, _token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        let mut previousNumerator = self.previousNumerator.write().await;
        let mut rng = rand::thread_rng();
        let numerator_range = (
            max(
                (self.ratio.numerator.get() as f64 * (1.0 - VARIATION_RANGE)).round() as u64,
                (previousNumerator.get() as f64 * (1.0 - NEXT_VALUE_VARIATION_RANGE)).round()
                    as u64,
            ),
            min(
                (self.ratio.numerator.get() as f64 * (1.0 + VARIATION_RANGE)).round() as u64,
                (previousNumerator.get() as f64 * (1.0 + NEXT_VALUE_VARIATION_RANGE)).round()
                    as u64,
            ),
        );

        let new_numerator = NonZeroU64::new(rng.gen_range(numerator_range.0..=numerator_range.1))
            .unwrap_or(self.ratio.numerator);
        let adjusted_ratio = BaseTokenAPIRatio {
            numerator: new_numerator,
            denominator: self.ratio.denominator,
            ratio_timestamp: chrono::Utc::now(),
        };
        *previousNumerator = new_numerator;

        Ok(adjusted_ratio)
    }
}
