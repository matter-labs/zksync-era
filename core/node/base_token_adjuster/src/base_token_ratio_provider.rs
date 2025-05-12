use std::{
    fmt::Debug,
    num::NonZeroU64,
    sync::{Arc, RwLock},
};

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_config::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_fee_model::BaseTokenRatioProvider;
use zksync_types::fee_model::BaseTokenConversionRatio;

#[derive(Debug, Clone)]
pub struct DBBaseTokenRatioProvider {
    pub pool: ConnectionPool<Core>,
    pub latest_ratio: Arc<RwLock<BaseTokenConversionRatio>>,
    config: BaseTokenAdjusterConfig,
}

impl DBBaseTokenRatioProvider {
    pub async fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
    ) -> anyhow::Result<Self> {
        let fetcher = Self {
            pool,
            latest_ratio: Arc::default(),
            config,
        };
        fetcher.update_latest_price().await?;

        // TODO(PE-129): Implement latest ratio usability logic.

        tracing::debug!(
            "Starting the base token ratio provider with conversion ratio: {:?}",
            fetcher.latest_ratio
        );
        Ok(fetcher)
    }

    fn get_latest_ratio(&self) -> BaseTokenConversionRatio {
        *self.latest_ratio.read().unwrap()
    }

    pub async fn run(&self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.config.price_cache_update_interval());

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            // TODO(PE-129): Implement latest ratio usability logic.
            self.update_latest_price().await?;
        }

        tracing::info!("Stop request received, base_token_ratio_provider is shutting down");
        Ok(())
    }

    async fn update_latest_price(&self) -> anyhow::Result<()> {
        let latest_storage_ratio = self
            .pool
            .connection_tagged("db_base_token_ratio_provider")
            .await
            .context("Failed to obtain connection to the database")?
            .base_token_dal()
            .get_latest_ratio()
            .await;

        let ratio = match latest_storage_ratio {
            Ok(Some(latest_storage_price)) => BaseTokenConversionRatio {
                numerator: latest_storage_price.numerator,
                denominator: latest_storage_price.denominator,
            },
            Ok(None) => {
                // TODO(PE-136): Insert initial ratio from genesis.
                // Though the DB should be populated very soon after the server starts, it is possible
                // to have no ratios in the DB right after genesis. Having initial ratios in the DB
                // from the genesis stage will eliminate this possibility.
                tracing::warn!("No latest price found in the database. Using default ratio.");
                BaseTokenConversionRatio::default()
            }
            Err(err) => anyhow::bail!("Failed to get latest base token ratio: {:?}", err),
        };

        *self.latest_ratio.write().unwrap() = ratio;
        Ok(())
    }
}

#[async_trait]
impl BaseTokenRatioProvider for DBBaseTokenRatioProvider {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
        self.get_latest_ratio()
    }
}

// Struct for a no-op BaseTokenRatioProvider (conversion ratio is either always 1:1 or a forced ratio).
#[derive(Debug, Clone)]
pub struct NoOpRatioProvider {
    pub latest_ratio: BaseTokenConversionRatio,
}

impl NoOpRatioProvider {
    pub fn new(latest_ratio: BaseTokenConversionRatio) -> Self {
        Self { latest_ratio }
    }
}

impl Default for NoOpRatioProvider {
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
impl BaseTokenRatioProvider for NoOpRatioProvider {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
        self.latest_ratio
    }
}
