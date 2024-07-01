use std::{fmt::Debug, num::NonZeroU64, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use tokio::{sync::watch, time::sleep};
use zksync_dal::{ConnectionPool, Core, CoreDal, DalError};
use zksync_types::{base_token_ratio::BaseTokenRatio, fee_model::BaseTokenConversionRatio};

const CACHE_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

#[async_trait]
pub trait BaseTokenFetcher: Debug + Send + Sync {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio;
}

#[derive(Debug, Clone)]
pub struct DBBaseTokenFetcher {
    pub pool: ConnectionPool<Core>,
    pub latest_ratio: BaseTokenConversionRatio,
}

impl DBBaseTokenFetcher {
    pub async fn new(pool: ConnectionPool<Core>) -> anyhow::Result<Self> {
        let latest_storage_ratio = retry_get_latest_price(pool.clone()).await?;

        // TODO(PE-129): Implement latest ratio usability logic.
        let latest_ratio = BaseTokenConversionRatio {
            numerator: latest_storage_ratio.numerator,
            denominator: latest_storage_ratio.denominator,
        };
        tracing::debug!(
            "Starting the base token fetcher with conversion ratio: {:?}",
            latest_ratio
        );
        Ok(Self { pool, latest_ratio })
    }

    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(CACHE_UPDATE_INTERVAL);

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            let latest_storage_ratio = get_latest_price(self.pool.clone())
                .await?
                .expect("No latest base token ratio found");

            // TODO(PE-129): Implement latest ratio usability logic.
            self.latest_ratio = BaseTokenConversionRatio {
                numerator: latest_storage_ratio.numerator,
                denominator: latest_storage_ratio.denominator,
            };
        }

        tracing::info!("Stop signal received, base_token_fetcher is shutting down");
        Ok(())
    }
}

async fn get_latest_price(pool: ConnectionPool<Core>) -> anyhow::Result<Option<BaseTokenRatio>> {
    pool.connection_tagged("db_base_token_fetcher")
        .await
        .context("Failed to obtain connection to the database")?
        .base_token_dal()
        .get_latest_ratio()
        .await
        .map_err(DalError::generalize)
}

async fn retry_get_latest_price(pool: ConnectionPool<Core>) -> anyhow::Result<BaseTokenRatio> {
    let sleep_duration = Duration::from_secs(1);
    let max_retries = 6; // should be enough time to allow fetching from external APIs & updating the DB upon init
    let mut attempts = 1;

    loop {
        match get_latest_price(pool.clone()).await {
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
                anyhow::bail!(
                    "Failed to get latest base token ratio with DAL error: {:?}",
                    err
                );
            }
        }
    }
}

#[async_trait]
impl BaseTokenFetcher for DBBaseTokenFetcher {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
        self.latest_ratio
    }
}

// Struct for a no-op BaseTokenFetcher (conversion ratio is either always 1:1 or a forced ratio).
#[derive(Debug, Clone)]
pub struct NoOpFetcher {
    pub latest_ratio: BaseTokenConversionRatio,
}

impl NoOpFetcher {
    pub fn new(latest_ratio: BaseTokenConversionRatio) -> Self {
        Self { latest_ratio }
    }
}

impl Default for NoOpFetcher {
    fn default() -> Self {
        Self {
            latest_ratio: BaseTokenConversionRatio {
                numerator: NonZeroU64::new(1).unwrap(),
                denominator: NonZeroU64::new(1).unwrap(),
            },
        }
    }
}

#[async_trait]
impl BaseTokenFetcher for NoOpFetcher {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
        self.latest_ratio
    }
}
