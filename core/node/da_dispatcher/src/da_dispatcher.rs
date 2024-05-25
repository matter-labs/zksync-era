use std::{future::Future, time::Duration};

use anyhow::Error;
use chrono::Utc;
use tokio::sync::watch;
use zksync_config::DADispatcherConfig;
use zksync_da_layers::DataAvailabilityInterface;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

use crate::metrics::METRICS;

#[derive(Debug)]
pub struct DataAvailabilityDispatcher {
    client: Box<dyn DataAvailabilityInterface>,
    pool: ConnectionPool<Core>,
    config: DADispatcherConfig,
}

impl DataAvailabilityDispatcher {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: DADispatcherConfig,
        client: Box<dyn DataAvailabilityInterface>,
    ) -> Self {
        Self {
            pool,
            config,
            client,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        loop {
            let mut conn = pool.connection_tagged("da_dispatcher").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, da_dispatcher is shutting down");
                break;
            }

            if let Err(err) = self.dispatch(&mut conn).await {
                tracing::warn!("dispatch error {err:?}");
            }
            if let Err(err) = self.poll_for_inclusion(&mut conn).await {
                tracing::warn!("poll_for_inclusion error {err:?}");
            }

            drop(conn);
            tokio::time::sleep(self.config.polling_interval()).await;
        }
        Ok(())
    }

    async fn dispatch(&self, conn: &mut Connection<'_, Core>) -> anyhow::Result<()> {
        let batches = conn
            .blocks_dal()
            .get_ready_for_da_dispatch_l1_batches(self.config.query_rows_limit() as usize)
            .await?;

        for batch in batches {
            let dispatch_latency = METRICS.blob_dispatch_latency.start();
            let dispatch_response = retry(self.config.max_retries(), || {
                self.client
                    .dispatch_blob(batch.l1_batch_number.0, batch.pubdata.clone())
            })
            .await
            .map_err(Error::msg)?;
            dispatch_latency.observe();

            conn.blocks_dal()
                .insert_l1_batch_da(batch.l1_batch_number, dispatch_response.blob_id)
                .await?;

            METRICS
                .last_known_l1_batch
                .set(batch.l1_batch_number.0 as usize);
            METRICS.blob_size.observe(batch.pubdata.len());
            tracing::info!(
                "Dispatched a DA for batch_number: {}",
                batch.l1_batch_number
            );
        }

        Ok(())
    }

    async fn poll_for_inclusion(&self, conn: &mut Connection<'_, Core>) -> anyhow::Result<()> {
        let storage_da = conn.blocks_dal().get_da_blob_awaiting_inclusion().await?;

        if let Some(storage_da) = storage_da {
            let inclusion_data = self
                .client
                .get_inclusion_data(storage_da.blob_id.clone().unwrap())
                .await
                .map_err(Error::msg)?;

            if let Some(inclusion_data) = inclusion_data {
                conn.blocks_dal()
                    .save_l1_batch_inclusion_data(
                        L1BatchNumber(storage_da.l1_batch_number as u32),
                        inclusion_data.data,
                    )
                    .await?;

                METRICS.inclusion_latency.observe(Duration::from_secs(
                    (Utc::now().timestamp() - storage_da.created_at.timestamp()) as u64,
                ));

                tracing::info!(
                    "Received an inclusion data for a batch_number: {}",
                    storage_da.l1_batch_number
                );
            }
        }

        Ok(())
    }
}

async fn retry<T, E, Fut, F>(max_retries: u16, mut f: F) -> Result<T, E>
where
    E: std::fmt::Display,
    Fut: Future<Output = Result<T, E>>,
    F: FnMut() -> Fut,
{
    let mut retries = 1;
    let mut backoff = 1;
    loop {
        match f().await {
            Ok(result) => {
                METRICS.dispatch_call_retries.observe(retries as usize);
                return Ok(result);
            }
            Err(err) => {
                tracing::warn!(%err, "Failed DA dispatch request {retries}/{max_retries}, retrying.");
                if retries > max_retries {
                    return Err(err);
                }
                retries += 1;
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff *= 2;
            }
        }
    }
}
