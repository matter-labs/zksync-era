use std::{future::Future, sync::Arc, time::Duration};

use anyhow::Context;
use chrono::Utc;
use rand::Rng;
use tokio::sync::watch::Receiver;
use zksync_config::{configs::contracts::chain::L2Contracts, DADispatcherConfig};
use zksync_da_client::{
    types::{DAError, InclusionData},
    DataAvailabilityClient,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{l2_to_l1_log::L2ToL1Log, Address, L1BatchNumber, H256};

use crate::metrics::METRICS;

#[derive(Debug, Clone)]
pub struct DataAvailabilityDispatcher {
    client: Box<dyn DataAvailabilityClient>,
    pool: ConnectionPool<Core>,
    config: DADispatcherConfig,
    l2_contracts: L2Contracts,
    transitional_l2_da_validator_address: Option<Address>, // set only if inclusion_verification_transition_enabled is true
}

impl DataAvailabilityDispatcher {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: DADispatcherConfig,
        client: Box<dyn DataAvailabilityClient>,
        l2_contracts: L2Contracts,
    ) -> Self {
        Self {
            pool,
            config,
            client,
            l2_contracts,
            transitional_l2_da_validator_address: None,
        }
    }

    pub async fn run(mut self, mut stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        self.check_for_misconfiguration().await?;
        let self_arc_dispatch = Arc::new(self.clone());
        let self_arc_finality = Arc::new(self.clone());

        let mut stop_receiver_dispatch = stop_receiver.clone();
        let mut stop_receiver_finality = stop_receiver.clone();
        let mut stop_receiver_poll_for_inclusion = stop_receiver.clone();

        let dispatch_task = tokio::spawn(async move {
            loop {
                if *stop_receiver_dispatch.borrow() {
                    break;
                }

                if let Err(err) = self_arc_dispatch.dispatch().await {
                    tracing::error!("dispatch error {err:?}");
                }

                if tokio::time::timeout(
                    self_arc_dispatch.config.polling_interval,
                    stop_receiver_dispatch.changed(),
                )
                .await
                .is_ok()
                {
                    break;
                }
            }
        });

        let finality_task = tokio::spawn(async move {
            loop {
                if *stop_receiver_finality.borrow() {
                    break;
                }

                if let Err(err) = self_arc_finality.ensure_finality().await {
                    tracing::error!("poll_for_inclusion error {err:?}");
                }

                if tokio::time::timeout(
                    self_arc_finality.config.polling_interval,
                    stop_receiver_finality.changed(),
                )
                .await
                .is_ok()
                {
                    break;
                }
            }
        });

        let inclusion_task = tokio::spawn(async move {
            loop {
                if *stop_receiver_poll_for_inclusion.borrow() {
                    break;
                }

                if let Err(err) = self.poll_for_inclusion().await {
                    tracing::error!("poll_for_inclusion error {err:?}");
                }

                if tokio::time::timeout(
                    self.config.polling_interval,
                    stop_receiver_poll_for_inclusion.changed(),
                )
                .await
                .is_ok()
                {
                    break;
                }
            }
        });

        tokio::select! {
            _ = dispatch_task => {},
            _ = finality_task => {},
            _ = inclusion_task => {},
            _ = stop_receiver.changed() => {},
        }

        tracing::info!("Stop request received, da_dispatcher is shutting down");
        Ok(())
    }

    /// Dispatches the blobs to the data availability layer, and saves the dispatch_request_id in the database.
    async fn dispatch(&self) -> anyhow::Result<()> {
        let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
        let batches = conn
            .data_availability_dal()
            .get_ready_for_da_dispatch_l1_batches(self.config.max_rows_to_dispatch as usize)
            .await?;
        drop(conn);

        for batch in &batches {
            let dispatch_latency = METRICS.blob_dispatch_latency.start();
            let dispatch_response = retry(
                self.config.max_retries,
                batch.l1_batch_number,
                "DA dispatch",
                || {
                    self.client
                        .dispatch_blob(batch.l1_batch_number.0, batch.pubdata.clone())
                },
            )
            .await
            .with_context(|| {
                format!(
                    "failed to dispatch a blob with batch_number: {}, pubdata_len: {}",
                    batch.l1_batch_number,
                    batch.pubdata.len()
                )
            })?;
            let dispatch_latency_duration = dispatch_latency.observe();

            let sent_at = Utc::now();

            let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
            conn.data_availability_dal()
                .insert_l1_batch_da_request_id(
                    batch.l1_batch_number,
                    dispatch_response.request_id.as_str(),
                    sent_at.naive_utc(),
                    self.client.client_type().into_pubdata_type(),
                    Some(find_l2_da_validator_address(batch.system_logs.as_slice())?),
                )
                .await?;
            drop(conn);

            METRICS
                .last_dispatched_l1_batch
                .set(batch.l1_batch_number.0 as usize);
            METRICS.blob_size.observe(batch.pubdata.len());
            METRICS.sealed_to_dispatched_lag.observe(
                sent_at
                    .signed_duration_since(batch.sealed_at)
                    .to_std()
                    .context("sent_at has to be higher than sealed_at")?,
            );
            tracing::info!(
                "Dispatched a DA for batch_number: {}, pubdata_size: {}, dispatch_latency: {dispatch_latency_duration:?}",
                batch.l1_batch_number,
                batch.pubdata.len(),
            );
        }

        // We don't need to report this metric every iteration, only once when the balance is changed
        if !batches.is_empty() {
            let client_arc = Arc::new(self.client.clone_boxed());

            tokio::spawn(async move {
                let balance = client_arc
                    .balance()
                    .await
                    .context("Unable to retrieve DA operator balance");

                match balance {
                    Ok(balance) => {
                        METRICS.operator_balance.set(balance);
                    }
                    Err(err) => {
                        tracing::error!("{err}")
                    }
                }
            });
        }

        Ok(())
    }

    async fn ensure_finality(&self) -> anyhow::Result<()> {
        let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
        let blob = conn
            .data_availability_dal()
            .get_first_da_blob_awaiting_finality()
            .await?;
        drop(conn);

        let Some(blob) = blob else {
            return Ok(());
        };

        // TODO: add metrics for finality latency
        let finality_response = self
            .client
            .ensure_finality(blob.dispatch_request_id.clone(), blob.sent_at)
            .await;

        match finality_response {
            Ok(None) => {
                // Not final yet, do nothing
            }
            Ok(Some(finality_response)) => {
                let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
                conn.data_availability_dal()
                    .set_blob_id(blob.l1_batch_number, finality_response.blob_id.as_str())
                    .await?;

                tracing::info!(
                    "Finality check for a batch_number: {} is successful",
                    blob.l1_batch_number
                );
            }
            Err(err) => {
                tracing::warn!(
                    "Finality check for a batch_number: {} failed with an error: {}",
                    blob.l1_batch_number,
                    err.error
                );

                // remove the entry from the database to resend the blob again
                let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
                conn.data_availability_dal()
                    .remove_data_availability_entry(blob.l1_batch_number)
                    .await?;
            }
        }

        Ok(())
    }

    /// Polls the data availability layer for inclusion data, and saves it in the database.
    async fn poll_for_inclusion(&self) -> anyhow::Result<()> {
        if self.config.inclusion_verification_transition_enabled {
            if let Some(l2_da_validator) = self.transitional_l2_da_validator_address {
                // Setting dummy inclusion data to the batches with the old L2 DA validator is necessary
                // for the transition process. We want to avoid the situation when the batch was sealed
                // but not dispatched to DA layer before transition, and then it will have an inclusion
                // data that is meant to be used with the new L2 DA validator. This will cause the
                // mismatch during the CommitBatches transaction. To avoid that we need to commit that
                // batch with dummy inclusion data during transition.
                let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
                conn.data_availability_dal()
                    .set_dummy_inclusion_data_for_old_batches(l2_da_validator)
                    .await?;
            }

            return Ok(());
        }

        let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
        let blob_info = conn
            .data_availability_dal()
            .get_first_da_blob_awaiting_inclusion()
            .await?;
        drop(conn);

        tracing::debug!("Getting inclusion data for blob_info: {:?}...", blob_info);

        let Some(blob_info) = blob_info else {
            return Ok(());
        };

        let inclusion_data = if self.config.use_dummy_inclusion_data {
            Some(InclusionData { data: vec![] })
        } else {
            let Some(blob_id) = blob_info.blob_id else {
                anyhow::bail!(
                    "Blob ID is not set for batch_number: {}",
                    blob_info.l1_batch_number
                );
            };

            self.client
                .get_inclusion_data(blob_id.as_str())
                .await
                .with_context(|| {
                    format!(
                        "failed to get inclusion data for blob_id: {}, batch_number: {}",
                        blob_id, blob_info.l1_batch_number
                    )
                })?
        };

        let Some(inclusion_data) = inclusion_data else {
            return Ok(());
        };

        let mut conn = self.pool.connection_tagged("da_dispatcher").await?;
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

        tracing::info!(
            "Received an inclusion data for a batch_number: {}, inclusion_latency_seconds: {}",
            blob_info.l1_batch_number,
            inclusion_latency.num_seconds()
        );

        Ok(())
    }

    async fn check_for_misconfiguration(&mut self) -> anyhow::Result<()> {
        if self.config.inclusion_verification_transition_enabled {
            self.transitional_l2_da_validator_address = Some(
                self.l2_contracts
                    .da_validator_addr
                    .context("L2 DA validator address is not set")?,
            );
        }
        Ok(())
    }
}

async fn retry<T, Fut, F>(
    max_retries: u16,
    batch_number: L1BatchNumber,
    action_name: &str,
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
                tracing::warn!(
                    %err,
                    "Failed {action_name} {retries}/{} for batch {batch_number}, retrying in {} milliseconds.",
                    max_retries+1,
                    sleep_duration.as_millis()
                );
                tokio::time::sleep(sleep_duration).await;

                backoff_secs = (backoff_secs * 2).min(128); // cap the back-off at 128 seconds
            }
        }
    }
}

pub fn find_l2_da_validator_address(system_logs: &[L2ToL1Log]) -> anyhow::Result<Address> {
    Ok(system_logs
        .iter()
        .find(|log| {
            log.key
                == H256::from_low_u64_be(u64::from(
                    zksync_system_constants::L2_DA_VALIDATOR_OUTPUT_HASH_KEY,
                ))
        })
        .context("L2 DA validator address log is missing")?
        .value
        .into())
}
