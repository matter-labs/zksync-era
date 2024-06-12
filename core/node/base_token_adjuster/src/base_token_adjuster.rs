use std::{
    future::Future,
    time::{Duration, Instant},
};

use anyhow::Context;
use chrono::{DateTime, NaiveDateTime, Utc};
use rand::Rng;
use tokio::sync::watch;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{BigDecimal, ConnectionPool, Core, CoreDal, BaseTokenDal};
use zksync_types::{L1BatchNumber, base_token_price::BaseTokenAPIPrice};

use crate::metrics::METRICS;

#[derive(Debug)]
pub struct BaseTokenAdjuster {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    // PriceAPIClient
}

impl BaseTokenAdjuster {
    pub fn new(pool: ConnectionPool<Core>, config: BaseTokenAdjusterConfig) -> Self {
        Self { pool, config }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, base_token_adjuster is shutting down");
                break;
            }

            let start_time = Instant::now();

            match self.fetch_new_ratio().await {
                Ok(new_ratio) => {
                    if let Err(err) = self
                        .persist_ratio(&new_ratio, &pool)
                        .await
                    {
                        tracing::error!("Error persisting ratio: {:?}", err);
                    }

                    if let Err(err) = self
                        .maybe_update_l1(&new_numerator, &new_denominator, ratio_timestamp)
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
        let ratio_timestamp = DateTime::from(Utc::now());

        Ok(BaseTokenAPIPrice {
            numerator: new_numerator,
            denominator: new_denominator,
            ratio_timestamp: ratio_timestamp,
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
                api_price.numerator,
                api_price.denominator,
                api_price.ratio_timestamp.naive_utc(),
            )
            .await?;
        drop(conn);

        Ok(())
    }

    // TODO: async fn maybe_update_l1()

    // Sleep for the remaining duration of the polling period
    async fn sleep_until_next_fetch(&self, start_time: Instant) {
        let elapsed_time = start_time.elapsed();
        let sleep_duration = if elapsed_time
            >= Duration::from_millis(&self.config.price_polling_interval_ms as u64)
        {
            Duration::from_secs(0)
        } else {
            Duration::from_secs(self.config.external_fetching_poll_period) - elapsed_time
        };

        tokio::time::sleep(sleep_duration).await;
    }
}
