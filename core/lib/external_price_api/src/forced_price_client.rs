use std::{
    cmp::{max, min},
    num::NonZeroU64,
};

use async_trait::async_trait;
use rand::Rng;
use tokio::sync::Mutex;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenApiRatio, fee_model::ConversionRatio};

use crate::{APIToken, PriceApiClient};

// Struct for a forced price "client" (conversion ratio is always a configured "forced" ratio).
#[derive(Debug)]
pub struct ForcedPriceClient {
    ratio: BaseTokenApiRatio,
    previous_numerator: Mutex<NonZeroU64>,
    fluctuation: Option<u32>,
    next_value_fluctuation: u32,
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
        let next_value_fluctuation = forced_price_client_config
            .next_value_fluctuation
            .clamp(0, 100);

        let ratio = if numerator < 100 && fluctuation.is_some_and(|f| f > 0) {
            // If numerator is too small we need to multiply by 100 to make sure fluctuations can be applied
            BaseTokenApiRatio {
                ratio: ConversionRatio {
                    numerator: NonZeroU64::new(numerator * 100).unwrap(),
                    denominator: NonZeroU64::new(denominator * 100).unwrap(),
                },
                ratio_timestamp: chrono::Utc::now(),
            }
        } else {
            BaseTokenApiRatio {
                ratio: ConversionRatio {
                    numerator: NonZeroU64::new(numerator).unwrap(),
                    denominator: NonZeroU64::new(denominator).unwrap(),
                },
                ratio_timestamp: chrono::Utc::now(),
            }
        };

        Self {
            ratio,
            previous_numerator: Mutex::new(NonZeroU64::new(numerator).unwrap()),
            fluctuation,
            next_value_fluctuation,
        }
    }
}

#[async_trait]
impl PriceApiClient for ForcedPriceClient {
    /// Returns the configured ratio with fluctuation applied if enabled
    async fn fetch_ratio(&self, token: APIToken) -> anyhow::Result<BaseTokenApiRatio> {
        // For non-ERC20 tokens we return default ratio, this way this client only applies to the CBT<->ETH not ZK<->ETH.
        if !matches!(token, APIToken::ERC20(_)) {
            return Ok(BaseTokenApiRatio::identity());
        }

        if let Some(fluctation) = self.fluctuation {
            let mut previous_numerator = self.previous_numerator.lock().await;
            let mut rng = rand::thread_rng();
            let numerator_range = (
                max(
                    (self.ratio.ratio.numerator.get() as f64 * (1.0 - (fluctation as f64 / 100.0)))
                        .round() as u64,
                    (previous_numerator.get() as f64
                        * (1.0 - (self.next_value_fluctuation as f64 / 100.0)))
                        .round() as u64,
                ),
                min(
                    (self.ratio.ratio.numerator.get() as f64 * (1.0 + (fluctation as f64 / 100.0)))
                        .round() as u64,
                    (previous_numerator.get() as f64
                        * (1.0 + (self.next_value_fluctuation as f64 / 100.0)))
                        .round() as u64,
                ),
            );

            let new_numerator =
                NonZeroU64::new(rng.gen_range(numerator_range.0..=numerator_range.1))
                    .unwrap_or(self.ratio.ratio.numerator);
            let adjusted_ratio = BaseTokenApiRatio {
                ratio: ConversionRatio {
                    numerator: new_numerator,
                    denominator: self.ratio.ratio.denominator,
                },
                ratio_timestamp: chrono::Utc::now(),
            };
            *previous_numerator = new_numerator;
            Ok(adjusted_ratio)
        } else {
            Ok(self.ratio)
        }
    }
}
