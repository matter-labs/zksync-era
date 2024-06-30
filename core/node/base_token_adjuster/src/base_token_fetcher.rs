use std::{fmt::Debug, num::NonZeroU64, time::Duration};

use tokio::{sync::watch, time::sleep};
use zksync_config::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{base_token_ratio::BaseTokenRatio, fee_model::BaseTokenConversionRatio};

const CACHE_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct BaseTokenFetcher {
    pub pool: Option<ConnectionPool<Core>>,
    pub latest_ratio: BaseTokenConversionRatio,
    pub config: BaseTokenAdjusterConfig,
}

impl BaseTokenFetcher {
    pub fn new(pool: Option<ConnectionPool<Core>>, config: BaseTokenAdjusterConfig) -> Self {
        let latest_ratio = BaseTokenConversionRatio {
            numerator: config.initial_numerator,
            denominator: config.initial_denominator,
        };
        tracing::debug!(
            "Starting the base token fetcher with conversion ratio: {:?}",
            latest_ratio
        );
        Self {
            pool,
            latest_ratio,
            config,
        }
    }

    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(CACHE_UPDATE_INTERVAL);

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            let latest_storage_ratio = self.retry_get_latest_price().await?;

            // TODO(PE-129): Implement latest ratio usability logic.
            self.latest_ratio = BaseTokenConversionRatio {
                numerator: latest_storage_ratio.numerator,
                denominator: latest_storage_ratio.denominator,
            };
        }

        tracing::info!("Stop signal received, base_token_fetcher is shutting down");
        Ok(())
    }

    async fn retry_get_latest_price(&self) -> anyhow::Result<BaseTokenRatio> {
        let sleep_duration = Duration::from_secs(1);
        let max_retries = 5; // should be enough time to allow fetching from external APIs & updating the DB upon init
        let mut attempts = 1;

        loop {
            let mut conn = self
                .pool
                .as_ref()
                .expect("Connection pool is not set")
                .connection_tagged("base_token_fetcher")
                .await
                .expect("Failed to obtain connection to the database");

            let dal_result = conn.base_token_dal().get_latest_ratio().await;

            drop(conn); // Don't sleep with your connections.

            match dal_result {
                Ok(Some(last_storage_price)) => {
                    return Ok(last_storage_price);
                }
                Ok(None) if attempts <= max_retries => {
                    tracing::warn!(
                        "Attempt {}/{} found no latest base token ratio. Retrying in {} seconds...",
                        attempts,
                        max_retries,
                        sleep_duration.as_secs()
                    );
                    sleep(sleep_duration).await;
                    attempts += 1;
                }
                Ok(None) => {
                    anyhow::bail!(
                        "No latest base token ratio found after {} attempts",
                        max_retries
                    );
                }
                Err(err) => {
                    anyhow::bail!("Failed to get latest base token ratio: {:?}", err);
                }
            }
        }
    }

    // TODO(PE-129): Implement latest ratio usability logic.
    pub fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
        self.latest_ratio
    }
}

// Default impl for a no-op BaseTokenFetcher (conversion ratio is always 1:1).
impl Default for BaseTokenFetcher {
    fn default() -> Self {
        Self {
            pool: None,
            latest_ratio: BaseTokenConversionRatio {
                numerator: NonZeroU64::new(1).unwrap(),
                denominator: NonZeroU64::new(1).unwrap(),
            },
            config: BaseTokenAdjusterConfig {
                price_polling_interval_ms: None,
                initial_numerator: NonZeroU64::new(1).unwrap(),
                initial_denominator: NonZeroU64::new(1).unwrap(),
            },
        }
    }
}
