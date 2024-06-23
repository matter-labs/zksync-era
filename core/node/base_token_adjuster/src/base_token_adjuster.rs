use std::{
    fmt::Debug,
    ops::Div,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::Utc;
use rand::Rng;
use tokio::{sync::watch, time::sleep};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{
    base_token_price::BaseTokenAPIPrice,
    fee_model::{FeeModelConfigV2, FeeParams, FeeParamsV2},
};

#[async_trait]
pub trait BaseTokenAdjuster: Debug + Send + Sync {
    /// Returns the last ratio cached by the adjuster and ensure it's still usable.
    async fn maybe_convert_to_base_token(&self, params: FeeParams) -> anyhow::Result<FeeParams>;

    /// Return configured symbol of the base token.
    fn get_base_token(&self) -> &str;
}

#[derive(Debug, Clone)]
/// BaseTokenAdjuster implementation for the main node (not the External Node). TODO (PE-137): impl APIBaseTokenAdjuster
pub struct MainNodeBaseTokenAdjuster {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
}

impl MainNodeBaseTokenAdjuster {
    pub fn new(pool: ConnectionPool<Core>, config: BaseTokenAdjusterConfig) -> Self {
        Self { pool, config }
    }

    /// Main loop for the base token adjuster.
    /// Orchestrates fetching new ratio, persisting it, and updating L1.
    pub async fn run(&mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, base_token_adjuster is shutting down");
                break;
            }

            let start_time = Instant::now();

            match self.fetch_new_ratio().await {
                Ok(new_ratio) => match self.persist_ratio(&new_ratio, &pool).await {
                    Ok(id) => {
                        if let Err(err) = self.maybe_update_l1(&new_ratio, id).await {
                            tracing::error!("Error updating L1 ratio: {:?}", err);
                        }
                    }
                    Err(err) => tracing::error!("Error persisting ratio: {:?}", err),
                },
                Err(err) => tracing::error!("Error fetching new ratio: {:?}", err),
            }

            self.sleep_until_next_fetch(start_time).await;
        }
        Ok(())
    }

    // TODO (PE-135): Use real API client to fetch new ratio through self.PriceAPIClient & mock for tests.
    //  For now, these hard coded values are also hard coded in the integration tests.
    async fn fetch_new_ratio(&self) -> anyhow::Result<BaseTokenAPIPrice> {
        let new_base_token_price = BigDecimal::from(1);
        let new_eth_price = BigDecimal::from(100000);
        let ratio_timestamp = Utc::now();

        Ok(BaseTokenAPIPrice {
            base_token_price: new_base_token_price,
            eth_price: new_eth_price,
            ratio_timestamp,
        })
    }

    async fn persist_ratio(
        &self,
        api_price: &BaseTokenAPIPrice,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<usize> {
        let mut conn = pool
            .connection_tagged("base_token_adjuster")
            .await
            .context("Failed to obtain connection to the database")?;

        let id = conn
            .base_token_dal()
            .insert_token_price(
                &api_price.base_token_price,
                &api_price.eth_price,
                &api_price.ratio_timestamp.naive_utc(),
            )
            .await?;
        drop(conn);

        Ok(id)
    }

    // TODO (PE-128): Complete L1 update flow.
    async fn maybe_update_l1(
        &self,
        _new_ratio: &BaseTokenAPIPrice,
        _id: usize,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn retry_get_latest_price(&self) -> anyhow::Result<BigDecimal> {
        let mut retry_delay = 1; // seconds
        let max_retries = 5;
        let mut attempts = 1;

        loop {
            let mut conn = self
                .pool
                .connection_tagged("base_token_adjuster")
                .await
                .expect("Failed to obtain connection to the database");

            let result = conn.base_token_dal().get_latest_price().await;

            drop(conn);

            if let Ok(last_storage_price) = result {
                return Ok(last_storage_price
                    .base_token_price
                    .div(&last_storage_price.eth_price));
            } else {
                if attempts >= max_retries {
                    break;
                }
                let sleep_duration = Duration::from_secs(retry_delay)
                    .mul_f32(rand::thread_rng().gen_range(0.8..1.2));
                tracing::warn!(
                    "Attempt {}/{} failed to get latest base token price, retrying in {} seconds...",
                    attempts, max_retries, sleep_duration.as_secs()
                );
                sleep(sleep_duration).await;
                retry_delay *= 2;
                attempts += 1;
            }
        }
        anyhow::bail!(
            "Failed to get latest base token price after {} attempts",
            max_retries
        );
    }

    // Sleep for the remaining duration of the polling period
    async fn sleep_until_next_fetch(&self, start_time: Instant) {
        let elapsed_time = start_time.elapsed();
        let sleep_duration = if elapsed_time >= self.config.price_polling_interval() {
            Duration::from_secs(0)
        } else {
            self.config.price_polling_interval() - elapsed_time
        };

        tokio::time::sleep(sleep_duration).await;
    }
}

#[async_trait]
impl BaseTokenAdjuster for MainNodeBaseTokenAdjuster {
    // TODO (PE-129): Implement latest ratio usability logic.
    async fn maybe_convert_to_base_token(&self, params: FeeParams) -> anyhow::Result<FeeParams> {
        let base_token = self.get_base_token();

        if base_token == "ETH" {
            return Ok(params);
        }

        // Retries are necessary for the initial setup, where prices may not yet be persisted.
        let latest_ratio = self.retry_get_latest_price().await?;

        if let FeeParams::V2(params_v2) = params {
            Ok(FeeParams::V2(convert_to_base_token(
                params_v2,
                latest_ratio,
            )))
        } else {
            panic!("Custom base token is not supported for V1 fee model")
        }
    }

    /// Return configured symbol of the base token. If not configured, return "ETH".
    fn get_base_token(&self) -> &str {
        match &self.config.base_token {
            Some(base_token) => base_token.as_str(),
            None => "ETH",
        }
    }
}

/// Converts the fee parameters to the base token using the latest ratio fetched from the DB.
fn convert_to_base_token(params: FeeParamsV2, base_token_to_eth: BigDecimal) -> FeeParamsV2 {
    let FeeParamsV2 {
        config,
        l1_gas_price,
        l1_pubdata_price,
    } = params;

    let convert_price = |price_in_wei: u64| -> u64 {
        let converted_price_bd = BigDecimal::from(price_in_wei) * base_token_to_eth.clone();
        match converted_price_bd.to_u64() {
            Some(converted_price) => converted_price,
            None => {
                if converted_price_bd > BigDecimal::from(u64::MAX) {
                    tracing::warn!(
                        "Conversion to base token price failed: converted price is too large: {}",
                        converted_price_bd
                    );
                } else {
                    tracing::error!(
                        "Conversion to base token price failed: converted price is not a valid u64: {}",
                        converted_price_bd
                    );
                }
                u64::MAX
            }
        }
    };

    let l1_gas_price_converted = convert_price(l1_gas_price);
    let l1_pubdata_price_converted = convert_price(l1_pubdata_price);
    let minimal_l2_gas_price_converted = convert_price(config.minimal_l2_gas_price);

    FeeParamsV2 {
        config: FeeModelConfigV2 {
            minimal_l2_gas_price: minimal_l2_gas_price_converted,
            ..config
        },
        l1_gas_price: l1_gas_price_converted,
        l1_pubdata_price: l1_pubdata_price_converted,
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MockBaseTokenAdjuster {
    last_ratio: BigDecimal,
    base_token: String,
}

impl MockBaseTokenAdjuster {
    pub fn new(last_ratio: BigDecimal, base_token: String) -> Self {
        Self {
            last_ratio,
            base_token,
        }
    }
}

impl Default for MockBaseTokenAdjuster {
    fn default() -> Self {
        Self {
            last_ratio: BigDecimal::from(1),
            base_token: "ETH".to_string(),
        }
    }
}

#[async_trait]
impl BaseTokenAdjuster for MockBaseTokenAdjuster {
    async fn maybe_convert_to_base_token(&self, params: FeeParams) -> anyhow::Result<FeeParams> {
        // LOG THE PARAMS
        tracing::info!("Params: {:?}", params);
        match self.get_base_token() {
            "ETH" => Ok(params),
            _ => {
                if let FeeParams::V2(params_v2) = params {
                    Ok(FeeParams::V2(convert_to_base_token(
                        params_v2,
                        self.last_ratio.clone(),
                    )))
                } else {
                    panic!("Custom base token is not supported for V1 fee model")
                }
            }
        }
    }

    fn get_base_token(&self) -> &str {
        &self.base_token
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bigdecimal::BigDecimal;
    use zksync_types::fee_model::{FeeModelConfigV2, FeeParamsV2};

    use super::*;

    #[test]
    fn test_convert_to_base_token() {
        let base_fee_model_config = FeeModelConfigV2 {
            minimal_l2_gas_price: 0,
            // All the below are unaffected by this flow.
            compute_overhead_part: 1.0,
            pubdata_overhead_part: 1.0,
            batch_overhead_l1_gas: 1,
            max_gas_per_batch: 1,
            max_pubdata_per_batch: 1,
        };

        struct TestCase {
            name: &'static str,
            base_token_to_eth: BigDecimal,
            input_minimal_l2_gas_price: u64,
            input_l1_gas_price: u64,
            input_l1_pubdata_price: u64,
            expected_minimal_l2_gas_price: u64,
            expected_l1_gas_price: u64,
            expected_l1_pubdata_price: u64,
        }

        let test_cases = vec![
            TestCase {
                name: "1 ETH = 2 BaseToken",
                base_token_to_eth: BigDecimal::from(2),
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 2000,
                expected_l1_gas_price: 4000,
                expected_l1_pubdata_price: 6000,
            },
            TestCase {
                name: "1 ETH = 0.5 BaseToken",
                base_token_to_eth: BigDecimal::from_str("0.5").unwrap(),
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 500,
                expected_l1_gas_price: 1000,
                expected_l1_pubdata_price: 1500,
            },
            TestCase {
                name: "1 ETH = 1 BaseToken",
                base_token_to_eth: BigDecimal::from(1),
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 1000,
                expected_l1_gas_price: 2000,
                expected_l1_pubdata_price: 3000,
            },
            TestCase {
                name: "Small conversion - 1 ETH - 1_000_000 BaseToken",
                base_token_to_eth: BigDecimal::from_str("0.000001").unwrap(),
                input_minimal_l2_gas_price: 1_000_000,
                input_l1_gas_price: 2_000_000,
                input_l1_pubdata_price: 3_000_000,
                expected_minimal_l2_gas_price: 1,
                expected_l1_gas_price: 2,
                expected_l1_pubdata_price: 3,
            },
            TestCase {
                name: "Large conversion - 1 ETH = 0.000001 BaseToken",
                base_token_to_eth: BigDecimal::from_str("1000000").unwrap(),
                input_minimal_l2_gas_price: 1,
                input_l1_gas_price: 2,
                input_l1_pubdata_price: 3,
                expected_minimal_l2_gas_price: 1_000_000,
                expected_l1_gas_price: 2_000_000,
                expected_l1_pubdata_price: 3_000_000,
            },
            TestCase {
                name: "Fractional conversion ratio",
                base_token_to_eth: BigDecimal::from_str("1.123456789").unwrap(),
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 1123,
                expected_l1_gas_price: 2246,
                expected_l1_pubdata_price: 3370,
            },
            TestCase {
                name: "Zero conversion ratio",
                base_token_to_eth: BigDecimal::from(0),
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 0,
                expected_l1_gas_price: 0,
                expected_l1_pubdata_price: 0,
            },
            TestCase {
                name: "Conversion ratio too large so clamp down to u64::MAX",
                base_token_to_eth: BigDecimal::from(u64::MAX),
                input_minimal_l2_gas_price: 2,
                input_l1_gas_price: 2,
                input_l1_pubdata_price: 2,
                expected_minimal_l2_gas_price: u64::MAX,
                expected_l1_gas_price: u64::MAX,
                expected_l1_pubdata_price: u64::MAX,
            },
        ];

        for case in test_cases {
            let input_params = FeeParamsV2 {
                config: FeeModelConfigV2 {
                    minimal_l2_gas_price: case.input_minimal_l2_gas_price,
                    ..base_fee_model_config
                },
                l1_gas_price: case.input_l1_gas_price,
                l1_pubdata_price: case.input_l1_pubdata_price,
            };

            let result = convert_to_base_token(input_params, case.base_token_to_eth.clone());

            assert_eq!(
                result.config.minimal_l2_gas_price, case.expected_minimal_l2_gas_price,
                "Test case '{}' failed: minimal_l2_gas_price mismatch",
                case.name
            );
            assert_eq!(
                result.l1_gas_price, case.expected_l1_gas_price,
                "Test case '{}' failed: l1_gas_price mismatch",
                case.name
            );
            assert_eq!(
                result.l1_pubdata_price, case.expected_l1_pubdata_price,
                "Test case '{}' failed: l1_pubdata_price mismatch",
                case.name
            );
        }
    }
}
