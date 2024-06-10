use std::{future::Future, time::Duration};

use anyhow::Context;
use chrono::{NaiveDateTime, Utc};
use rand::Rng;
use tokio::sync::watch;
// use zksync_config::BaseTokenAdjusterConfig;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_da_client::{
    types::{DAError, IsTransient},
    DataAvailabilityClient,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

use crate::metrics::METRICS;

#[derive(Debug)]
pub struct BaseTokenAdjuster {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
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

            if let Err(err) = self.adjust(&pool).await {
                tracing::warn!("adjust error {err:?}");
            }
        }
        Ok(())
    }

    /// adjust me
    async fn adjust(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut conn = pool.connection_tagged("da_dispatcher").await?;
        let batches = conn
            .data_availability_dal()
            .get_ready_for_da_dispatch_l1_batches(self.config.query_rows_limit() as usize)
            .await?;
        drop(conn);

        Ok(())
    }
}

// TODO: Interesting to note age of price used by each batch.
