use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use chrono::Utc;
use rand::Rng;
use tokio::{sync::watch, time::sleep};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{
    base_token_ratio::BaseTokenAPIRatio, fee_model::BaseTokenConversionRatio, Address,
    SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
};

#[async_trait]
pub trait BaseTokenAdjuster: Debug + Send + Sync {
    /// Returns the last ratio cached by the adjuster and ensure it's still usable.
    async fn get_conversion_ratio(&self) -> anyhow::Result<BaseTokenConversionRatio>;
}

#[derive(Debug, Clone)]
/// BaseTokenAdjuster implementation for the main node (not the External Node). TODO (PE-137): impl APIBaseTokenAdjuster
pub struct MainNodeBaseTokenAdjuster {
    pool: ConnectionPool<Core>,
    base_token_l1_address: Address,
    config: BaseTokenAdjusterConfig,
}

impl MainNodeBaseTokenAdjuster {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token_l1_address: Address,
    ) -> Self {
        Self {
            pool,
            config,
            base_token_l1_address,
        }
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
                Ok(new_ratio) => {
                    match self.persist_ratio(&new_ratio, &pool).await {
                        Ok(_) => {
                            // TODO(PE-128): Update L1 ratio
                        }
                        Err(err) => tracing::error!("Error persisting ratio: {:?}", err),
                    }
                }
                Err(err) => tracing::error!("Error fetching new ratio: {:?}", err),
            }

            self.sleep_until_next_fetch(start_time).await;
        }
        Ok(())
    }

    // TODO (PE-135): Use real API client to fetch new ratio through self.PriceAPIClient & mock for tests.
    //  For now, these hard coded values are also hard coded in the integration tests.
    async fn fetch_new_ratio(&self) -> anyhow::Result<BaseTokenAPIRatio> {
        let ratio_timestamp = Utc::now();

        Ok(BaseTokenAPIRatio {
            numerator: 1,
            denominator: 100000,
            ratio_timestamp,
        })
    }

    async fn persist_ratio(
        &self,
        api_price: &BaseTokenAPIRatio,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<usize> {
        let mut conn = pool
            .connection_tagged("base_token_adjuster")
            .await
            .context("Failed to obtain connection to the database")?;

        let id = conn
            .base_token_dal()
            .insert_token_ratio(
                api_price.numerator,
                api_price.denominator,
                &api_price.ratio_timestamp.naive_utc(),
            )
            .await?;
        drop(conn);

        Ok(id)
    }

    async fn retry_get_latest_price(&self) -> anyhow::Result<BaseTokenConversionRatio> {
        let mut retry_delay = 1; // seconds
        let max_retries = 5;
        let mut attempts = 1;

        loop {
            let mut conn = self
                .pool
                .connection_tagged("base_token_adjuster")
                .await
                .expect("Failed to obtain connection to the database");

            let result = conn.base_token_dal().get_latest_ratio().await;

            drop(conn);

            if let Ok(last_storage_price) = result {
                return Ok(BaseTokenConversionRatio {
                    numerator: last_storage_price.numerator,
                    denominator: last_storage_price.denominator,
                });
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

    /// Compare with SHARED_BRIDGE_ETHER_TOKEN_ADDRESS
    fn is_base_token_eth(&self) -> bool {
        self.base_token_l1_address == SHARED_BRIDGE_ETHER_TOKEN_ADDRESS
    }
}

#[async_trait]
impl BaseTokenAdjuster for MainNodeBaseTokenAdjuster {
    async fn get_conversion_ratio(&self) -> anyhow::Result<BaseTokenConversionRatio> {
        let is_eth = self.is_base_token_eth();

        if is_eth {
            return Ok(BaseTokenConversionRatio {
                numerator: 1,
                denominator: 1,
            });
        }

        // Retries are necessary for the initial setup, where prices may not yet be persisted.
        self.retry_get_latest_price().await

        // TODO(PE-129): Implement latest ratio usability logic.
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MockBaseTokenAdjuster {
    last_ratio: BaseTokenConversionRatio,
}

impl MockBaseTokenAdjuster {
    pub fn new(last_ratio: BaseTokenConversionRatio) -> Self {
        Self { last_ratio }
    }
}

impl Default for MockBaseTokenAdjuster {
    fn default() -> Self {
        Self {
            last_ratio: BaseTokenConversionRatio {
                numerator: 1,
                denominator: 1,
            },
        }
    }
}

#[async_trait]
impl BaseTokenAdjuster for MockBaseTokenAdjuster {
    async fn get_conversion_ratio(&self) -> anyhow::Result<BaseTokenConversionRatio> {
        Ok(self.last_ratio)
    }
}
