use std::{
    fmt::Debug,
    ops::Div,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::watch;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{BigDecimal, ConnectionPool, Core, CoreDal};
use zksync_types::base_token_price::BaseTokenAPIPrice;

#[async_trait]
pub trait BaseTokenAdjuster: Debug + Send + Sync {
    /// Returns the last ratio cached by the adjuster and ensure it's still usable.
    async fn get_last_ratio_and_check_usability<'a>(&'a self) -> BigDecimal;

    /// Return configured symbol of the base token.
    fn get_base_token(&self) -> &str;
}

#[derive(Debug)]
/// BaseTokenAdjuster implementation for the main node (not the External Node).
pub struct MainNodeBaseTokenAdjuster {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
}

impl MainNodeBaseTokenAdjuster {
    pub fn new(pool: ConnectionPool<Core>, config: BaseTokenAdjusterConfig) -> Self {
        Self { pool, config }
    }

    /// Main loop for the base token adjuster.
    /// Orchestrates fetching new ratio, persisting it, and updating L1.
    pub async fn run(&mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, base_token_adjuster is shutting down");
                break;
            }

            let start_time = Instant::now();

            match self.fetch_new_ratio().await {
                Ok(new_ratio) => match self.persist_ratio(&new_ratio, &pool).await {
                    Ok(id) => {
                        if let Err(err) = self.maybe_update_l1(&new_ratio, id).await {
                            tracing::error!("Error updating L1 ratio: {:?}", err);
                        }
                    }
                    Err(err) => tracing::error!("Error persisting ratio: {:?}", err),
                },
                Err(err) => tracing::error!("Error fetching new ratio: {:?}", err),
            }

            self.sleep_until_next_fetch(start_time).await;
        }
        Ok(())
    }

    // TODO (PE-135): Use real API client to fetch new ratio through self.PriceAPIClient & mock for tests.
    //  For now, these hard coded values are also hard coded in the integration tests.
    async fn fetch_new_ratio(&self) -> anyhow::Result<BaseTokenAPIPrice> {
        let new_numerator = BigDecimal::from(1);
        let new_denominator = BigDecimal::from(100);
        let ratio_timestamp = Utc::now();

        Ok(BaseTokenAPIPrice {
            numerator: new_numerator,
            denominator: new_denominator,
            ratio_timestamp,
        })
    }

    async fn persist_ratio(
        &self,
        api_price: &BaseTokenAPIPrice,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<usize> {
        let mut conn = pool
            .connection_tagged("base_token_adjuster")
            .await
            .context("Failed to obtain connection to the database")?;

        let id = conn
            .base_token_dal()
            .insert_token_price(
                &api_price.numerator,
                &api_price.denominator,
                &api_price.ratio_timestamp.naive_utc(),
            )
            .await?;
        drop(conn);

        Ok(id)
    }

    // TODO (PE-128): Complete L1 update flow.
    async fn maybe_update_l1(
        &self,
        _new_ratio: &BaseTokenAPIPrice,
        _id: usize,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    // Sleep for the remaining duration of the polling period
    async fn sleep_until_next_fetch(&self, start_time: Instant) {
        let elapsed_time = start_time.elapsed();
        let sleep_duration = if elapsed_time >= self.config.price_polling_interval() {
            Duration::from_secs(0)
        } else {
            self.config.price_polling_interval() - elapsed_time
        };

        tokio::time::sleep(sleep_duration).await;
    }
}

#[async_trait]
impl BaseTokenAdjuster for MainNodeBaseTokenAdjuster {
    // TODO (PE-129): Implement latest ratio usability logic.
    async fn get_last_ratio_and_check_usability<'a>(&'a self) -> BigDecimal {
        let mut conn = self
            .pool
            .connection_tagged("base_token_adjuster")
            .await
            .expect("Failed to obtain connection to the database");

        let last_storage_ratio = conn
            .base_token_dal()
            .get_latest_price()
            .await
            .expect("Failed to get latest base token price");
        drop(conn);

        let last_ratio = last_storage_ratio
            .numerator
            .div(&last_storage_ratio.denominator);

        last_ratio
    }

    /// Return configured symbol of the base token. If not configured, return "ETH".
    fn get_base_token(&self) -> &str {
        match &self.config.base_token {
            Some(base_token) => base_token.as_str(),
            None => "ETH",
        }
    }
}
