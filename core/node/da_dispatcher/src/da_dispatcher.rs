use std::{collections::HashSet, future::Future, sync::Arc, time::Duration};

use anyhow::Context;
use chrono::Utc;
use futures::future::join_all;
use rand::Rng;
use tokio::sync::{mpsc, watch::Receiver, Mutex, Notify};
use zksync_config::{configs::da_dispatcher::DEFAULT_MAX_CONCURRENT_REQUESTS, DADispatcherConfig};
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
        let (tx, mut rx) = mpsc::channel(
            self.config
                .max_concurrent_requests
                .unwrap_or(DEFAULT_MAX_CONCURRENT_REQUESTS) as usize,
        );

        let next_expected_batch = Arc::new(Mutex::new(None));

        let stop_receiver_clone = stop_receiver.clone();
        let pool_clone = self.pool.clone();
        let config_clone = self.config.clone();
        let next_expected_batch_clone = next_expected_batch.clone();
        let pending_blobs_reader = tokio::spawn(async move {
            // Used to avoid sending the same batch multiple times
            let mut pending_batches = HashSet::new();
            loop {
                if *stop_receiver_clone.borrow() {
                    tracing::info!("Stop signal received, da_dispatcher is shutting down");
                    break;
                }

                let mut conn = pool_clone.connection_tagged("da_dispatcher").await?;
                let batches = conn
                    .data_availability_dal()
                    .get_ready_for_da_dispatch_l1_batches(
                        config_clone.max_rows_to_dispatch() as usize
                    )
                    .await?;
                drop(conn);
                for batch in batches {
                    if pending_batches.contains(&batch.l1_batch_number.0) {
                        continue;
                    }

                    // This should only happen once.
                    // We can't assume that the first batch is always 1 because the dispatcher can be restarted
                    // and resume from a different batch.
                    let mut next_expected_batch_lock = next_expected_batch_clone.lock().await;
                    if next_expected_batch_lock.is_none() {
                        next_expected_batch_lock.replace(batch.l1_batch_number);
                    }

                    pending_batches.insert(batch.l1_batch_number.0);
                    METRICS.blobs_pending_dispatch.inc_by(1);
                    tx.send(batch).await?;
                }

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Ok::<(), anyhow::Error>(())
        });

        let pool = self.pool.clone();
        let config = self.config.clone();
        let client = self.client.clone();
        let request_semaphore = self.request_semaphore.clone();
        let notifier = Arc::new(Notify::new());
        let pending_blobs_sender = tokio::spawn(async move {
            let mut spawned_requests = vec![];
            let notifier = notifier.clone();
            loop {
                if *stop_receiver.borrow() {
                    break;
                }

                let batch = match rx.recv().await {
                    Some(batch) => batch,
                    None => continue, // Should never happen
                };

                // Block until we can send the request
                let permit = request_semaphore.clone().acquire_owned().await?;

                let client = client.clone();
                let pool = pool.clone();
                let config = config.clone();
                let next_expected_batch = next_expected_batch.clone();
                let notifier = notifier.clone();
                let request = tokio::spawn(async move {
                    let _permit = permit; // move permit into scope
                    let dispatch_latency = METRICS.blob_dispatch_latency.start();
                    let dispatch_response =
                        retry(config.max_retries(), batch.l1_batch_number, || {
                            client.dispatch_blob(batch.l1_batch_number.0, batch.pubdata.clone())
                        })
                        .await
                        .with_context(|| {
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

                    Ok::<(), anyhow::Error>(())
                });
                spawned_requests.push(request);
            }
            join_all(spawned_requests).await;
            Ok::<(), anyhow::Error>(())
        });

        let results = join_all(vec![pending_blobs_reader, pending_blobs_sender]).await;
        for result in results {
            result??;
        }
        Ok(())
    }

    async fn inclusion_poller(&self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel(
            self.config
                .max_concurrent_requests
                .unwrap_or(DEFAULT_MAX_CONCURRENT_REQUESTS) as usize,
        );

        let stop_receiver_clone = stop_receiver.clone();
        let pool_clone = self.pool.clone();
        let pending_inclusion_reader = tokio::spawn(async move {
            let mut pending_inclusions = HashSet::new();
            loop {
                if *stop_receiver_clone.borrow() {
                    break;
                }

                let mut conn = pool_clone.connection_tagged("da_dispatcher").await?;
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
                    tx.send(blob_info).await?;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Ok::<(), anyhow::Error>(())
        });

        let pool = self.pool.clone();
        let config = self.config.clone();
        let client = self.client.clone();
        let semaphore = self.request_semaphore.clone();
        let pending_inclusion_sender = tokio::spawn(async move {
            let mut spawned_requests = vec![];
            loop {
                if *stop_receiver.borrow() {
                    break;
                }
                let blob_info = match rx.recv().await {
                    Some(blob_info) => blob_info,
                    None => continue, // Should never happen
                };

                // Block until we can send the request
                let permit = semaphore.clone().acquire_owned().await?;

                let client = client.clone();
                let pool = pool.clone();
                let config = config.clone();
                let request = tokio::spawn(async move {
                    let _permit = permit; // move permit into scope
                    let inclusion_data = if config.use_dummy_inclusion_data() {
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

                    Ok::<(), anyhow::Error>(())
                });
                spawned_requests.push(request);
            }
            join_all(spawned_requests).await;
            Ok::<(), anyhow::Error>(())
        });

        let results = join_all(vec![pending_inclusion_reader, pending_inclusion_sender]).await;
        for result in results {
            result??;
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
