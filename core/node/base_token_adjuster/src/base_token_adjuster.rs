use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use tokio::sync::watch;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{BigDecimal, ConnectionPool, Core, CoreDal};
use zksync_types::base_token_price::{BaseTokenAPIPrice, BaseTokenPrice};

pub trait BaseTokenAdjuster: Debug + Send + Sync {
    fn get_last_ratio(&self) -> Option<BaseTokenPrice>;
}

#[derive(Debug)]
pub struct NodeBaseTokenAdjuster {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    // PriceAPIClient
}

impl NodeBaseTokenAdjuster {
    pub fn new(pool: ConnectionPool<Core>, config: BaseTokenAdjusterConfig) -> Self {
        Self { pool, config }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, base_token_adjuster is shutting down");
                break;
            }

            let start_time = Instant::now();

            match self.fetch_new_ratio().await {
                Ok(new_ratio) => {
                    if let Err(err) = self.persist_ratio(&new_ratio, &pool).await {
                        tracing::error!("Error persisting ratio: {:?}", err);
                    }

                    if let Err(err) = self
                        .maybe_update_l1(
                            &new_ratio.numerator,
                            &new_ratio.denominator,
                            &new_ratio.ratio_timestamp,
                        )
                        .await
                    {
                        tracing::error!("Error updating L1 ratio: {:?}", err);
                    }
                }
                Err(err) => tracing::error!("Error fetching new ratio: {:?}", err),
            }

            self.sleep_until_next_fetch(start_time).await;
        }
        Ok(())
    }

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
    ) -> anyhow::Result<()> {
        let mut conn = pool.connection_tagged("base_token_adjuster").await?;
        conn.base_token_dal()
            .insert_token_price(
                &api_price.numerator,
                &api_price.denominator,
                &api_price.ratio_timestamp.naive_utc(),
            )
            .await?;
        drop(conn);

        Ok(())
    }

    async fn maybe_update_l1(
        &self,
        _numerator: &BigDecimal,
        _denominator: &BigDecimal,
        _ratio_timestamp: &DateTime<Utc>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    // TODO: async fn maybe_update_l1()

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

impl BaseTokenAdjuster for NodeBaseTokenAdjuster {
    fn get_last_ratio(&self) -> Option<BaseTokenPrice> {
        None
    }
}
