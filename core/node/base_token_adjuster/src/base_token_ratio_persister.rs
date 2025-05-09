use std::{fmt::Debug, sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::{sync::watch, time::sleep};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_external_price_api::PriceAPIClient;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::{
    base_token_l1_behaviour::BaseTokenL1Behaviour,
    metrics::{OperationResult, OperationResultLabels, METRICS},
};

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    base_token_address: Address,
    price_api_client: Arc<dyn PriceAPIClient>,
    l1_behaviour: BaseTokenL1Behaviour,
}

impl BaseTokenRatioPersister {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token_address: Address,
        price_api_client: Arc<dyn PriceAPIClient>,
        l1_behaviour: BaseTokenL1Behaviour,
    ) -> Self {
        Self {
            pool,
            config,
            base_token_address,
            price_api_client,
            l1_behaviour,
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
                tracing::warn!(
                    "Error in the base_token_ratio_persister loop interaction {}",
                    err
                );
                if self.config.halt_on_error {
                    return Err(err)
                        .context("Failed to execute a base_token_ratio_persister loop iteration");
                }
            }
        }

        tracing::info!("Stop request received, base_token_ratio_persister is shutting down");
        Ok(())
    }

    async fn loop_iteration(&mut self) -> anyhow::Result<()> {
        // TODO(PE-148): Consider shifting retry upon adding external API redundancy.
        let new_ratio = self.retry_fetch_ratio().await?;
        self.persist_ratio(new_ratio).await?;
        self.l1_behaviour.update_l1(new_ratio).await
    }

    async fn retry_fetch_ratio(&self) -> anyhow::Result<BaseTokenAPIRatio> {
        let sleep_duration = self.config.price_fetching_sleep_duration();
        let max_retries = self.config.price_fetching_max_attempts;
        let mut last_error = None;

        for attempt in 0..max_retries {
            let start_time = Instant::now();
            match self
                .price_api_client
                .fetch_ratio(self.base_token_address)
                .await
            {
                Ok(ratio) => {
                    METRICS.external_price_api_latency[&OperationResultLabels {
                        result: OperationResult::Success,
                    }]
                        .observe(start_time.elapsed());
                    METRICS
                        .ratio
                        .set((ratio.numerator.get() as f64) / (ratio.denominator.get() as f64));
                    return Ok(ratio);
                }
                Err(err) => {
                    tracing::warn!(
                        "Attempt {}/{} to fetch ratio from external price api failed with err: {}. Retrying...",
                        attempt,
                        max_retries,
                        err
                    );
                    last_error = Some(err);
                    METRICS.external_price_api_latency[&OperationResultLabels {
                        result: OperationResult::Failure,
                    }]
                        .observe(start_time.elapsed());
                    sleep(sleep_duration).await;
                }
            }
        }
        let error_message = "Failed to fetch base token ratio after multiple attempts";
        Err(last_error
            .map(|x| x.context(error_message))
            .unwrap_or_else(|| anyhow::anyhow!(error_message)))
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
