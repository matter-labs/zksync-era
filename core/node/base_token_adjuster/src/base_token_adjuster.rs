use std::{fmt::Debug, num::NonZero};

use anyhow::Context as _;
use chrono::Utc;
use tokio::sync::watch;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::base_token_ratio::BaseTokenAPIRatio;

#[derive(Debug, Clone)]
/// BaseTokenAdjuster implementation for the main node (not the External Node). TODO (PE-137): impl APIBaseTokenAdjuster
pub struct BaseTokenConversionPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
}

impl BaseTokenConversionPersister {
    pub fn new(pool: ConnectionPool<Core>, config: BaseTokenAdjusterConfig) -> Self {
        Self { pool, config }
    }

    /// Main loop for the base token adjuster.
    /// Orchestrates fetching new ratio, persisting it, and updating L1.
    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.config.price_polling_interval());
        let pool = self.pool.clone();

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            let new_ratio = self.fetch_new_ratio().await?;
            self.persist_ratio(&new_ratio, &pool).await?;
            // TODO(PE-128): Update L1 ratio
        }

        tracing::info!("Stop signal received, eth_watch is shutting down");
        Ok(())
    }

    // TODO (PE-135): Use real API client to fetch new ratio through self.PriceAPIClient & mock for tests.
    //  For now, these are hard coded dummy values.
    async fn fetch_new_ratio(&self) -> anyhow::Result<BaseTokenAPIRatio> {
        let ratio_timestamp = Utc::now();

        Ok(BaseTokenAPIRatio {
            numerator: NonZero::new(1).unwrap(),
            denominator: NonZero::new(100000).unwrap(),
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
            .await
            .context("Failed to insert token ratio into the database")?;

        Ok(id)
    }
}
