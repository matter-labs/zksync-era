use std::{fmt::Debug, sync::Arc, time::Duration};

use anyhow::Context as _;
use tokio::{
    sync::{watch, Mutex},
    time::sleep,
};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_external_price_api::PriceAPIClient;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    base_token_address: Address,
    price_api_client: Arc<Mutex<dyn PriceAPIClient>>,
}

impl BaseTokenRatioPersister {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token_address: Address,
        price_api_client: Arc<Mutex<dyn PriceAPIClient>>,
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

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            if let Err(err) = self.loop_iteration().await {
                return Err(err)
                    .context("Failed to execute a base_token_ratio_persister loop iteration");
            }
        }

        tracing::info!("Stop signal received, base_token_ratio_persister is shutting down");
        Ok(())
    }

    async fn loop_iteration(&self) -> anyhow::Result<()> {
        // TODO(PE-148): Consider shifting retry upon adding external API redundancy.
        let new_ratio = self.retry_fetch_ratio().await?;

        self.persist_ratio(new_ratio).await?;
        // TODO(PE-128): Update L1 ratio

        Ok(())
    }

    async fn retry_fetch_ratio(&self) -> anyhow::Result<BaseTokenAPIRatio> {
        let sleep_duration = Duration::from_secs(1);
        let max_retries = 5;
        let mut attempts = 0;

        loop {
            let mut client = self.price_api_client.lock().await;

            match client.fetch_ratio(self.base_token_address).await {
                Ok(ratio) => {
                    return Ok(ratio);
                }
                Err(err) if attempts < max_retries => {
                    attempts += 1;
                    tracing::warn!(
                        "Attempt {}/{} to fetch ratio from coingecko failed with err: {}. Retrying...",
                        attempts,
                        max_retries,
                        err
                    );
                    sleep(sleep_duration).await;
                }
                Err(err) => {
                    return Err(err)
                        .context("Failed to fetch base token ratio after multiple attempts");
                }
            }
        }
    }

    async fn persist_ratio(&self, api_ratio: BaseTokenAPIRatio) -> anyhow::Result<usize> {
        let mut conn = self
            .pool
            .connection_tagged("base_token_ratio_persister")
            .await
            .context("Failed to obtain connection to the database")?;

        let id = conn
            .base_token_dal()
            .insert_token_ratio(
                api_ratio.numerator,
                api_ratio.denominator,
                &api_ratio.ratio_timestamp.naive_utc(),
            )
            .await
            .context("Failed to insert base token ratio into the database")?;

        Ok(id)
    }
}
