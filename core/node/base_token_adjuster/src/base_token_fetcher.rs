use std::{fmt::Debug, num::NonZeroU64, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::fee_model::BaseTokenConversionRatio;

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
        let mut fetcher = Self {
            pool,
            latest_ratio: BaseTokenConversionRatio::default(),
        };
        fetcher.latest_ratio = fetcher.get_latest_price().await?;

        // TODO(PE-129): Implement latest ratio usability logic.

        tracing::debug!(
            "Starting the base token fetcher with conversion ratio: {:?}",
            fetcher.latest_ratio
        );
        Ok(fetcher)
    }

    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(CACHE_UPDATE_INTERVAL);

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            let latest_storage_ratio = self.get_latest_price().await?;

            // TODO(PE-129): Implement latest ratio usability logic.
            self.latest_ratio = BaseTokenConversionRatio {
                numerator: latest_storage_ratio.numerator,
                denominator: latest_storage_ratio.denominator,
            };
        }

        tracing::info!("Stop signal received, base_token_fetcher is shutting down");
        Ok(())
    }

    async fn get_latest_price(&self) -> anyhow::Result<BaseTokenConversionRatio> {
        let latest_storage_ratio = self
            .pool
            .connection_tagged("db_base_token_fetcher")
            .await
            .context("Failed to obtain connection to the database")?
            .base_token_dal()
            .get_latest_ratio()
            .await;

        match latest_storage_ratio {
            Ok(Some(latest_storage_price)) => Ok(BaseTokenConversionRatio {
                numerator: latest_storage_price.numerator,
                denominator: latest_storage_price.denominator,
            }),
            Ok(None) => {
                // TODO(PE-136): Insert initial ratio from genesis.
                // Though the DB should be populated very soon after the server starts, it is possible
                // to have no ratios in the DB right after genesis. Having initial ratios in the DB
                // from the genesis stage will eliminate this possibility.
                tracing::error!("No latest price found in the database. Using default ratio.");
                Ok(BaseTokenConversionRatio::default())
            }
            Err(err) => anyhow::bail!("Failed to get latest base token ratio: {:?}", err),
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
