use std::{fmt::Debug, num::NonZeroU64, sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::{sync::watch, time::sleep};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_external_price_api::PriceApiClient;
use zksync_types::{base_token_ratio::BaseTokenApiRatio, Address};

use crate::{
    base_token_l1_behaviour::BaseTokenL1Behaviour,
    metrics::{OperationResult, OperationResultLabels, METRICS},
};

#[derive(Debug, Clone)]
pub enum BaseToken {
    ERC20(Address),
    ETH,
}

impl BaseToken {
    /// Interprets 0x00 and 0x01 addresses as ETH
    pub fn from_config_address(address: Address) -> Self {
        if address == Address::zero() || address == Address::from_low_u64_be(1) {
            Self::ETH
        } else {
            Self::ERC20(address)
        }
    }
}

/// When multiplying naivly to ratios based on u64 we can overflow. This function
/// scales down the ratios to prevent overflow (effectively loosing some precision).
/// In rare cases it may not be possible to sensible scale down (if ratio would be
/// more then 2^64 or less then 2^-64). In such cases we return an error.
fn safe_u64_fraction_mul(
    a: BaseTokenApiRatio,
    b: BaseTokenApiRatio,
) -> anyhow::Result<BaseTokenApiRatio> {
    let numerator = a.numerator.get() as u128 * b.numerator.get() as u128;
    let denominator = a.denominator.get() as u128 * b.denominator.get() as u128;

    // Check if either numerator or denominator exceeds u64::MAX
    if numerator > u64::MAX as u128 || denominator > u64::MAX as u128 {
        //
        let scaling_power = numerator
            .max(denominator)
            .leading_zeros()
            .saturating_sub(64);

        // Scale down both values
        let scaled_numerator = numerator >> scaling_power;
        let scaled_denominator = denominator >> scaling_power;

        // Ensure we don't have zeros after division
        let safe_numerator = NonZeroU64::new(
            scaled_numerator
                .try_into()
                .context(anyhow::anyhow!("Bad numerator after scaling down"))?,
        )
        .ok_or(anyhow::anyhow!("Scaled down numerator is zero"))?;
        let safe_denominator = NonZeroU64::new(
            scaled_denominator
                .try_into()
                .context(anyhow::anyhow!("Bad denominator after scaling down"))?,
        )
        .ok_or(anyhow::anyhow!("Scaled down denominator is zero"))?;

        Ok(BaseTokenApiRatio {
            numerator: safe_numerator,
            denominator: safe_denominator,
            ratio_timestamp: a.ratio_timestamp.max(b.ratio_timestamp),
        })
    } else {
        // No scaling needed, values are within u64 range
        Ok(BaseTokenApiRatio {
            numerator: NonZeroU64::new(numerator.try_into().unwrap()).unwrap(),
            denominator: NonZeroU64::new(denominator.try_into().unwrap()).unwrap(),
            ratio_timestamp: a.ratio_timestamp.max(b.ratio_timestamp),
        })
    }
}

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    base_token: BaseToken,
    sl_token: BaseToken,
    price_api_client: Arc<dyn PriceApiClient>,
    l1_behaviour: BaseTokenL1Behaviour,
}

impl BaseTokenRatioPersister {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token: BaseToken,
        sl_token: BaseToken,
        price_api_client: Arc<dyn PriceApiClient>,
        l1_behaviour: BaseTokenL1Behaviour,
    ) -> Self {
        Self {
            pool,
            config,
            base_token,
            sl_token,
            price_api_client,
            l1_behaviour,
        }
    }

    /// Main loop for the base token ratio persister.
    /// Orchestrates fetching a new ratio, persisting it, and conditionally updating the L1 with it.
    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.config.price_polling_interval);

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
        let base_to_eth = match self.base_token {
            BaseToken::ETH => BaseTokenApiRatio::identity(),
            BaseToken::ERC20(address) => self.retry_fetch_ratio(address).await?,
        };

        let sl_to_eth = match self.sl_token {
            BaseToken::ETH => BaseTokenApiRatio::identity(),
            BaseToken::ERC20(address) => self.retry_fetch_ratio(address).await?,
        };

        let new_ratio = safe_u64_fraction_mul(base_to_eth, sl_to_eth.reciprocal())?;
        METRICS
            .ratio
            .set((new_ratio.numerator.get() as f64) / (new_ratio.denominator.get() as f64));

        self.persist_ratio(new_ratio).await?;
        self.l1_behaviour.update_l1(new_ratio).await
    }

    async fn retry_fetch_ratio(&self, address: Address) -> anyhow::Result<BaseTokenApiRatio> {
        let sleep_duration = self.config.price_fetching_sleep;
        let max_retries = self.config.price_fetching_max_attempts;
        let mut last_error = None;

        for attempt in 0..max_retries {
            let start_time = Instant::now();
            match self.price_api_client.fetch_ratio(address).await {
                Ok(ratio) => {
                    METRICS.external_price_api_latency[&OperationResultLabels {
                        result: OperationResult::Success,
                    }]
                        .observe(start_time.elapsed());
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

    async fn persist_ratio(&self, api_ratio: BaseTokenApiRatio) -> anyhow::Result<usize> {
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
