use std::{fmt::Debug, num::NonZero, time::Duration};

use tokio::{sync::watch, time::sleep};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{base_token_ratio::BaseTokenRatio, fee_model::BaseTokenConversionRatio};

const CACHE_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
/// BaseTokenAdjuster implementation for the main node (not the External Node).
pub struct BaseTokenFetcher {
    pub pool: Option<ConnectionPool<Core>>,
    pub latest_ratio: Option<BaseTokenConversionRatio>,
}

impl BaseTokenFetcher {
    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(CACHE_UPDATE_INTERVAL);

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            let latest_storage_ratio = self.retry_get_latest_price().await?;

            // TODO(PE-129): Implement latest ratio usability logic.
            self.latest_ratio = Some(BaseTokenConversionRatio {
                numerator: latest_storage_ratio.numerator,
                denominator: latest_storage_ratio.denominator,
            });
        }

        tracing::info!("Stop signal received, eth_watch is shutting down");
        Ok(())
    }

    async fn retry_get_latest_price(&self) -> anyhow::Result<BaseTokenRatio> {
        let retry_delay = 1; // seconds
        let max_retries = 10;
        let mut attempts = 1;

        loop {
            let mut conn = self
                .pool
                .as_ref()
                .expect("Connection pool is not set")
                .connection_tagged("base_token_adjuster")
                .await
                .expect("Failed to obtain connection to the database");

            let dal_result = conn.base_token_dal().get_latest_ratio().await;

            drop(conn); // Don't sleep with your connections.

            match dal_result {
                Ok(Some(last_storage_price)) => {
                    return Ok(last_storage_price);
                }
                Ok(None) if attempts < max_retries => {
                    let sleep_duration = Duration::from_secs(retry_delay);
                    tracing::warn!(
                        "Attempt {}/{} found no latest base token price, retrying in {} seconds...",
                        attempts,
                        max_retries,
                        sleep_duration.as_secs()
                    );
                    sleep(sleep_duration).await;
                    attempts += 1;
                }
                Ok(None) => {
                    anyhow::bail!(
                        "No latest base token price found after {} attempts",
                        max_retries
                    );
                }
                Err(err) => {
                    anyhow::bail!("Failed to get latest base token price: {:?}", err);
                }
            }
        }
    }

    pub fn get_conversion_ratio(&self) -> Option<BaseTokenConversionRatio> {
        self.latest_ratio
    }
}

// Default impl for a No Op BaseTokenFetcher.
impl Default for BaseTokenFetcher {
    fn default() -> Self {
        Self {
            pool: None,
            latest_ratio: Some(BaseTokenConversionRatio {
                numerator: NonZero::new(1).unwrap(),
                denominator: NonZero::new(1).unwrap(),
            }),
        }
    }
}
