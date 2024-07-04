use std::{fmt::Debug, sync::Arc};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_external_price_api::PriceAPIClient;
use zksync_types::{base_token_ratio::BaseTokenAPIPrice, Address};

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    base_token_address: Address,
    price_api_client: Arc<dyn PriceAPIClient>,
}

impl BaseTokenRatioPersister {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token_address: Address,
        price_api_client: Arc<dyn PriceAPIClient>,
    ) -> Self {
        Self {
            pool,
            config,
            base_token_address,
            price_api_client,
        }
    }

    /// Main loop for the base token ratio persister.
    /// Orchestrates fetching a new ratio, persisting it, and conditionally updating the L1 with it.
    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.config.price_polling_interval());
        let pool = self.pool.clone();

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            let new_prices = self
                .price_api_client
                .fetch_prices(self.base_token_address)
                .await?;

            self.persist_ratio(new_prices, &pool).await?;
            // TODO(PE-128): Update L1 ratio
        }

        tracing::info!("Stop signal received, base_token_ratio_persister is shutting down");
        Ok(())
    }

    async fn persist_ratio(
        &self,
        api_price: BaseTokenAPIPrice,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<usize> {
        let mut conn = pool
            .connection_tagged("base_token_ratio_persister")
            .await
            .context("Failed to obtain connection to the database")?;

        let (numerator, denominator) = api_price.clone().get_fraction()?;
        let id = conn
            .base_token_dal()
            .insert_token_ratio(
                numerator,
                denominator,
                &api_price.ratio_timestamp.naive_utc(),
            )
            .await
            .context("Failed to insert base token ratio into the database")?;

        Ok(id)
    }
}
