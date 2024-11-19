use std::{collections::HashSet, future::Future, sync::Arc, time::Duration};

use anyhow::Context;
use chrono::Utc;
use rand::Rng;
use tokio::{
    sync::{
        watch::{self, Receiver},
        Mutex, Notify,
    },
    task::JoinSet,
};
use zksync_config::{
    configs::da_dispatcher::{DEFAULT_MAX_CONCURRENT_REQUESTS, DEFAULT_POLLING_INTERVAL_MS},
    DADispatcherConfig,
};
use zksync_da_client::{
    types::{DAError, InclusionData},
    DataAvailabilityClient,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

use crate::metrics::METRICS;

#[derive(Debug)]
pub struct DataAvailabilityDispatcher {
    client: Box<dyn DataAvailabilityClient>,
    pool: ConnectionPool<Core>,
    config: DADispatcherConfig,
    request_semaphore: Arc<tokio::sync::Semaphore>,
}

impl DataAvailabilityDispatcher {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: DADispatcherConfig,
        client: Box<dyn DataAvailabilityClient>,
    ) -> Self {
        let request_semaphore = Arc::new(tokio::sync::Semaphore::new(
            config
                .max_concurrent_requests
                .unwrap_or(DEFAULT_MAX_CONCURRENT_REQUESTS) as usize,
        ));
        Self {
            pool,
            config,
            client,
            request_semaphore,
        }
    }

    pub async fn run(self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        let subtasks = futures::future::join(
            async {
                if let Err(err) = self.dispatch_batches(stop_receiver.clone()).await {
                    tracing::error!("dispatch error {err:?}");
                }
            },
            async {
                if let Err(err) = self.inclusion_poller(stop_receiver.clone()).await {
                    tracing::error!("poll_for_inclusion error {err:?}");
                }
            },
        );

        tokio::select! {
            _ = subtasks => {},
        }
        Ok(())
    }

    async fn dispatch_batches(&self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        let next_expected_batch = Arc::new(Mutex::new(None));
        let mut pending_batches = HashSet::new();
        let mut dispatcher_tasks: JoinSet<anyhow::Result<()>> = JoinSet::new();

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let notifier = Arc::new(Notify::new());
        loop {
            if *stop_receiver.clone().borrow() {
                tracing::info!("Stop signal received, da_dispatcher is shutting down");
                break;
            }
            if *shutdown_rx.borrow() {
                tracing::error!("A blob dispatch failed, da_dispatcher is shutting down");
                break;
            }

            let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
            let batches = conn
                .data_availability_dal()
                .get_ready_for_da_dispatch_l1_batches(self.config.max_rows_to_dispatch() as usize)
                .await?;
            drop(conn);
            let shutdown_tx = shutdown_tx.clone();
            for batch in batches {
                if pending_batches.contains(&batch.l1_batch_number.0) {
                    continue;
                }

                // This should only happen once.
                // We can't assume that the first batch is always 1 because the dispatcher can be restarted
                // and resume from a different batch.
                let mut next_expected_batch_lock = next_expected_batch.lock().await;
                if next_expected_batch_lock.is_none() {
                    next_expected_batch_lock.replace(batch.l1_batch_number);
                }
                drop(next_expected_batch_lock);

                pending_batches.insert(batch.l1_batch_number.0);
                METRICS.blobs_pending_dispatch.inc_by(1);

                let request_semaphore = self.request_semaphore.clone();
                let client = self.client.clone();
                let config = self.config.clone();
                let notifier = notifier.clone();
                let shutdown_rx = shutdown_rx.clone();
                let shutdown_tx = shutdown_tx.clone();
                let next_expected_batch = next_expected_batch.clone();
                let pool = self.pool.clone();
                dispatcher_tasks.spawn(async move {
                    let permit = request_semaphore.clone().acquire_owned().await?;
                    let dispatch_latency = METRICS.blob_dispatch_latency.start();

                    let result = retry(config.max_retries(), batch.l1_batch_number, || {
                        client.dispatch_blob(batch.l1_batch_number.0, batch.pubdata.clone())
                    })
                    .await;
                    drop(permit);
                    if result.is_err() {
                        shutdown_tx.clone().send(true)?;
                        notifier.notify_waiters();
                    };

                    let dispatch_response = result.with_context(|| {
                        format!(
                            "failed to dispatch a blob with batch_number: {}, pubdata_len: {}",
                            batch.l1_batch_number,
                            batch.pubdata.len()
                        )
                    })?;
                    let dispatch_latency_duration = dispatch_latency.observe();

                    let sent_at = Utc::now().naive_utc();

                    // Before saving the blob in the database, we need to be sure that we are doing it
                    // in the correct order.
                    while next_expected_batch
                        .lock()
                        .await
                        .map_or(true, |next_expected_batch| {
                            batch.l1_batch_number > next_expected_batch
                        })
                    {
                        if *shutdown_rx.clone().borrow() {
                            return Err(anyhow::anyhow!(
                                "Batch {} failed to disperse: Shutdown signal received",
                                batch.l1_batch_number
                            ));
                        }
                        notifier.clone().notified().await;
                    }

                    let mut conn = pool.connection_tagged("da_dispatcher").await?;
                    conn.data_availability_dal()
                        .insert_l1_batch_da(
                            batch.l1_batch_number,
                            dispatch_response.blob_id.as_str(),
                            sent_at,
                        )
                        .await?;
                    drop(conn);

                    // Update the next expected batch number
                    next_expected_batch
                    .lock()
                    .await
                    .replace(batch.l1_batch_number + 1);
                    notifier.notify_waiters();

                    METRICS
                    .last_dispatched_l1_batch
                    .set(batch.l1_batch_number.0 as usize);
                    METRICS.blob_size.observe(batch.pubdata.len());
                    METRICS.blobs_dispatched.inc_by(1);
                    METRICS.blobs_pending_dispatch.dec_by(1);
                    tracing::info!(
                        "Dispatched a DA for batch_number: {}, pubdata_size: {}, dispatch_latency: {dispatch_latency_duration:?}",
                        batch.l1_batch_number,
                        batch.pubdata.len(),
                    );
                    Ok(())
                });
            }

            // Sleep so we prevent hammering the database
            tokio::time::sleep(Duration::from_secs(
                self.config
                    .polling_interval_ms
                    .unwrap_or(DEFAULT_POLLING_INTERVAL_MS) as u64,
            ))
            .await;
        }

        while let Some(next) = dispatcher_tasks.join_next().await {
            match next {
                Ok(value) => match value {
                    Ok(_) => (),
                    Err(err) => {
                        dispatcher_tasks.shutdown().await;
                        return Err(err);
                    }
                },
                Err(err) => {
                    dispatcher_tasks.shutdown().await;
                    return Err(err.into());
                }
            }
        }

        Ok(())
    }

    async fn inclusion_poller(&self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        let mut pending_inclusions = HashSet::new();
        let mut inclusion_tasks: JoinSet<anyhow::Result<()>> = JoinSet::new();

        loop {
            if *stop_receiver.borrow() {
                break;
            }

            let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
            let pending_blobs = conn
                .data_availability_dal()
                .get_da_blob_ids_awaiting_inclusion()
                .await?;
            drop(conn);

            for blob_info in pending_blobs.into_iter().flatten() {
                if pending_inclusions.contains(&blob_info.blob_id) {
                    continue;
                }
                pending_inclusions.insert(blob_info.blob_id.clone());

                let client = self.client.clone();
                let config = self.config.clone();
                let pool = self.pool.clone();
                let request_semaphore = self.request_semaphore.clone();
                inclusion_tasks.spawn(async move {
                    let inclusion_data = if config.use_dummy_inclusion_data() {
                        let _permit = request_semaphore.acquire_owned().await?;
                        client
                            .get_inclusion_data(blob_info.blob_id.as_str())
                            .await
                            .with_context(|| {
                                format!(
                                    "failed to get inclusion data for blob_id: {}, batch_number: {}",
                                    blob_info.blob_id, blob_info.l1_batch_number
                                )
                            })?
                        } else {
                            // if the inclusion verification is disabled, we don't need to wait for the inclusion
                            // data before committing the batch, so simply return an empty vector
                            Some(InclusionData { data: vec![] })
                        };

                    let Some(inclusion_data) = inclusion_data else {
                        return Ok(());
                    };

                    let mut conn = pool.connection_tagged("da_dispatcher").await?;
                    conn.data_availability_dal()
                        .save_l1_batch_inclusion_data(
                            L1BatchNumber(blob_info.l1_batch_number.0),
                            inclusion_data.data.as_slice(),
                        )
                    .await?;
                    drop(conn);

                    let inclusion_latency = Utc::now().signed_duration_since(blob_info.sent_at);
                    if let Ok(latency) = inclusion_latency.to_std() {
                        METRICS.inclusion_latency.observe(latency);
                    }
                    METRICS
                        .last_included_l1_batch
                        .set(blob_info.l1_batch_number.0 as usize);
                    METRICS.blobs_included.inc_by(1);

                    tracing::info!(
                        "Received an inclusion data for a batch_number: {}, inclusion_latency_seconds: {}",
                        blob_info.l1_batch_number,
                        inclusion_latency.num_seconds()
                    );
                    Ok(())
                });
            }

            // Sleep so we prevent hammering the database
            tokio::time::sleep(Duration::from_secs(
                self.config
                    .polling_interval_ms
                    .unwrap_or(DEFAULT_POLLING_INTERVAL_MS) as u64,
            ))
            .await;
        }

        while let Some(next) = inclusion_tasks.join_next().await {
            match next {
                Ok(_) => (),
                Err(e) => {
                    inclusion_tasks.shutdown().await;
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }
}

async fn retry<T, Fut, F>(
    max_retries: u16,
    batch_number: L1BatchNumber,
    mut f: F,
) -> Result<T, DAError>
where
    Fut: Future<Output = Result<T, DAError>>,
    F: FnMut() -> Fut,
{
    let mut retries = 1;
    let mut backoff_secs = 1;
    loop {
        match f().await {
            Ok(result) => {
                METRICS.dispatch_call_retries.observe(retries as usize);
                return Ok(result);
            }
            Err(err) => {
                if !err.is_retriable() || retries > max_retries {
                    return Err(err);
                }

                retries += 1;
                let sleep_duration = Duration::from_secs(backoff_secs)
                    .mul_f32(rand::thread_rng().gen_range(0.8..1.2));
                tracing::warn!(%err, "Failed DA dispatch request {retries}/{max_retries} for batch {batch_number}, retrying in {} milliseconds.", sleep_duration.as_millis());
                tokio::time::sleep(sleep_duration).await;

                backoff_secs = (backoff_secs * 2).min(128); // cap the back-off at 128 seconds
            }
        }
    }
}
