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
            fluctuation,
        }
    }
}

#[async_trait]
impl PriceAPIClient for ForcedPriceClient {
    // Returns a ratio which is 10% higher or lower than the configured forced ratio.
    async fn fetch_ratio(&self, _token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        if let Some(x) = self.fluctuation {
            if x != 0 {
                let mut rng = rand::thread_rng();

                let mut adjust_range = |value: NonZeroU64| {
                    let value_f64 = value.get() as f64;
                    let min = (value_f64 * (1.0 - x as f64 / 100.0)).round() as u64;
                    let max = (value_f64 * (1.0 + x as f64 / 100.0)).round() as u64;
                    rng.gen_range(min..=max)
                };
                let new_numerator = adjust_range(self.ratio.numerator);
                let new_denominator = adjust_range(self.ratio.denominator);

                return Ok(BaseTokenAPIRatio {
                    numerator: NonZeroU64::new(new_numerator).unwrap_or(self.ratio.numerator),
                    denominator: NonZeroU64::new(new_denominator).unwrap_or(self.ratio.denominator),
                    ratio_timestamp: chrono::Utc::now(),
                });
            }
        }
        Ok(self.ratio)
    }
}
