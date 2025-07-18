use std::{fmt::Debug, num::NonZeroU64, sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::{sync::watch, time::sleep};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_external_price_api::{APIToken, PriceApiClient};
use zksync_types::{
    base_token_ratio::BaseTokenApiRatio,
    fee_model::{BaseTokenConversionRatio, ConversionRatio},
};

use crate::{
    base_token_l1_behaviour::BaseTokenL1Behaviour,
    metrics::{OperationResult, OperationResultLabels, METRICS},
};

/// When multiplying naivly to ratios based on u64 we can overflow. This function
/// scales down the ratios to prevent overflow (effectively loosing some precision).
/// In rare cases it may not be possible to sensible scale down (if ratio would be
/// more then 2^64 or less then 2^-64). In such cases we return an error.
fn safe_u64_fraction_mul(
    a: BaseTokenApiRatio,
    b: BaseTokenApiRatio,
) -> anyhow::Result<BaseTokenApiRatio> {
    let numerator = a.ratio.numerator.get() as u128 * b.ratio.numerator.get() as u128;
    let denominator = a.ratio.denominator.get() as u128 * b.ratio.denominator.get() as u128;

    // We need to scale if numerator or denominator is bigger then u64.
    // If not the scaling factor is zero and does nothing
    let scaling_power = 64_u32.saturating_sub(numerator.max(denominator).leading_zeros());

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
        ratio: ConversionRatio {
            numerator: safe_numerator,
            denominator: safe_denominator,
        },
        ratio_timestamp: a.ratio_timestamp.max(b.ratio_timestamp),
    })
}

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    base_token: APIToken,
    sl_token: APIToken,
    price_api_client: Arc<dyn PriceApiClient>,
    l1_behaviour: BaseTokenL1Behaviour,
}

impl BaseTokenRatioPersister {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token: APIToken,
        sl_token: APIToken,
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
        let base_to_eth = self.retry_fetch_ratio(self.base_token).await?;

        let sl_to_eth = self.retry_fetch_ratio(self.sl_token).await?;

        let sl_ratio = safe_u64_fraction_mul(base_to_eth, sl_to_eth.reciprocal())?;
        METRICS.ratio.set(
            (sl_ratio.ratio.numerator.get() as f64) / (sl_ratio.ratio.denominator.get() as f64),
        );

        tracing::info!(
            "Base to SL ({:?}) ratio: {:?}, base ({:?}) to ETH ratio: {:?}",
            self.sl_token,
            sl_ratio.ratio,
            self.base_token,
            base_to_eth.ratio
        );

        // In database we persist the ratio needed for calculating L2 gas price from SL (L1 or Gateway) gas price
        self.persist_ratio(
            BaseTokenConversionRatio::new(base_to_eth.ratio, sl_ratio.ratio),
            sl_ratio.ratio_timestamp,
        )
        .await?;
        if !matches!(self.base_token, APIToken::Eth) {
            METRICS.ratio_l1.set(
                (base_to_eth.ratio.numerator.get() as f64)
                    / (base_to_eth.ratio.denominator.get() as f64),
            );
            self.l1_behaviour.update_l1(base_to_eth).await?
        }
        Ok(())
    }

    async fn retry_fetch_ratio(&self, token: APIToken) -> anyhow::Result<BaseTokenApiRatio> {
        let sleep_duration = self.config.price_fetching_sleep;
        let max_retries = self.config.price_fetching_max_attempts;
        let mut last_error = None;

        for attempt in 0..max_retries {
            let start_time = Instant::now();
            match self.price_api_client.fetch_ratio(token).await {
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

    async fn persist_ratio(
        &self,
        api_ratio: BaseTokenConversionRatio,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<usize> {
        let mut conn = self
            .pool
            .connection_tagged("base_token_ratio_persister")
            .await
            .context("Failed to obtain connection to the database")?;

        let id = conn
            .base_token_dal()
            .insert_token_ratio(api_ratio, &timestamp.naive_utc())
            .await
            .context("Failed to insert base token ratio into the database")?;

        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        num::NonZeroU64,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use anyhow::Result;
    use async_trait::async_trait;
    use chrono::Utc;
    use test_casing::test_casing;
    use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
    use zksync_dal::{ConnectionPool, Core, CoreDal};
    use zksync_external_price_api::{APIToken, PriceApiClient};
    use zksync_types::{base_token_ratio::BaseTokenApiRatio, fee_model::ConversionRatio, Address};

    use crate::*;

    // Mock for the PriceApiClient trait
    #[derive(Debug, Clone, Default)]
    struct MockPriceApiClient {
        // Map from token address to the ratio it should return
        ratios: Arc<Mutex<HashMap<Address, BaseTokenApiRatio>>>,
        // To simulate failures
        should_fail_count: Arc<Mutex<u64>>,
    }

    impl MockPriceApiClient {
        fn new() -> Self {
            Self::default()
        }

        fn set_ratio(&self, token_address: Address, ratio: BaseTokenApiRatio) {
            self.ratios.lock().unwrap().insert(token_address, ratio);
        }

        fn set_should_fail_count(&self, should_fail_count: u64) {
            *self.should_fail_count.lock().unwrap() = should_fail_count;
        }
    }

    #[async_trait]
    impl PriceApiClient for MockPriceApiClient {
        async fn fetch_ratio(&self, token: APIToken) -> Result<BaseTokenApiRatio> {
            if *self.should_fail_count.lock().unwrap() > 0 {
                *self.should_fail_count.lock().unwrap() -= 1;
                return Err(anyhow::anyhow!("Simulated API failure"));
            }

            let address = match token {
                APIToken::ERC20(address) => address,
                APIToken::ZK => ZK_ADDRESS,
                APIToken::Eth => return Ok(BaseTokenApiRatio::identity()),
            };

            self.ratios
                .lock()
                .unwrap()
                .get(&address)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Token ratio not found"))
        }
    }

    fn create_test_config() -> BaseTokenAdjusterConfig {
        BaseTokenAdjusterConfig {
            price_polling_interval: Duration::from_millis(100),
            price_fetching_max_attempts: 3,
            price_fetching_sleep: Duration::ZERO,
            halt_on_error: true,
            ..Default::default()
        }
    }

    fn create_test_ratio(numerator: u64, denominator: u64) -> BaseTokenApiRatio {
        BaseTokenApiRatio {
            ratio: ConversionRatio {
                numerator: NonZeroU64::new(numerator).unwrap(),
                denominator: NonZeroU64::new(denominator).unwrap(),
            },
            ratio_timestamp: Utc::now(),
        }
    }

    /// Check the database for inserted token ratios
    async fn verify_db_ratios(
        pool: &ConnectionPool<Core>,
        expected_num_ratio: u64,
        expected_denom_ratio: u64,
    ) {
        let mut conn = pool.connection().await.unwrap();
        let ratio = conn.base_token_dal().get_latest_ratio().await.unwrap();

        assert!(ratio.is_some(), "No token ratios found in database");

        // Check the latest ratio matches expected values
        let ratio = ratio.unwrap();
        assert_eq!(ratio.ratio.sl.numerator.get(), expected_num_ratio);
        assert_eq!(ratio.ratio.sl.denominator.get(), expected_denom_ratio);
    }

    const TOKEN_1_ADDRESS: Address = Address::repeat_byte(0x01);
    const TOKEN_1: APIToken = APIToken::ERC20(TOKEN_1_ADDRESS);
    const TOKEN_1_PRICE: (u64, u64) = (500, 1);
    const ZK_ADDRESS: Address = Address::repeat_byte(0x03);
    const ZK_PRICE: (u64, u64) = (1000, 1);
    const ZK: APIToken = APIToken::ZK;
    const ETH: APIToken = APIToken::Eth;

    // Test initialization function
    async fn init_test(
        base_token: APIToken,
        sl_token: APIToken,
    ) -> (
        ConnectionPool<Core>,
        Arc<MockPriceApiClient>,
        BaseTokenRatioPersister,
    ) {
        // Setup a real database pool
        let pool = ConnectionPool::<Core>::test_pool().await;

        // Setup mock for the price API client
        let mock_client = Arc::new(MockPriceApiClient::new());

        // Set up expected ratios in the mock client
        let base_to_eth_ratio = create_test_ratio(TOKEN_1_PRICE.0, TOKEN_1_PRICE.1);
        let sl_to_eth_ratio = create_test_ratio(ZK_PRICE.0, ZK_PRICE.1);

        mock_client.set_ratio(TOKEN_1_ADDRESS, base_to_eth_ratio);
        mock_client.set_ratio(ZK_ADDRESS, sl_to_eth_ratio);

        // Create the persister with real database pool
        let persister = BaseTokenRatioPersister::new(
            pool.clone(),
            create_test_config(),
            base_token,
            sl_token,
            mock_client.clone() as Arc<dyn PriceApiClient>,
            BaseTokenL1Behaviour::NoOp,
        );
        (pool, mock_client, persister)
    }

    #[test_casing(3, vec![(TOKEN_1, ZK, (500,1000)), (ETH, ZK, (1,1000)), (TOKEN_1, ETH, (500,1))])]
    #[tokio::test]
    async fn test_fetch_and_persist(
        base_token: APIToken,
        sl_token: APIToken,
        expected_db_ratio: (u64, u64),
    ) {
        // Setup test environment
        let (pool, _mock_client, mut persister) = init_test(base_token, sl_token).await;

        persister.loop_iteration().await.unwrap();

        // Verify that the correct ratio was stored in DB
        verify_db_ratios(&pool, expected_db_ratio.0, expected_db_ratio.1).await;
    }

    #[test_casing(3, vec![(TOKEN_1, ZK, (500,1000)), (ETH, ZK, (1,1000)), (TOKEN_1, ETH, (500,1))])]
    #[tokio::test]
    async fn test_retry_mechanism(
        base_token: APIToken,
        sl_token: APIToken,
        expected_db_ratio: (u64, u64),
    ) {
        // Setup test environment
        let (pool, mock_client, mut persister) = init_test(base_token, sl_token).await;

        mock_client.set_should_fail_count(1); // Fail once

        persister.loop_iteration().await.unwrap();

        // Verify that the correct ratio was eventually stored in DB
        verify_db_ratios(&pool, expected_db_ratio.0, expected_db_ratio.1).await;
    }
}
