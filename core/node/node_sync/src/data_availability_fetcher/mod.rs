//! Fetcher is responsible for getting DA information from the main node.

use std::time::Duration;

use anyhow::{bail, Context as _};
use serde::Serialize;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_da_client::DataAvailabilityClient;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    block::{L1BatchTreeData, L2BlockHeader},
    Address, L1BatchNumber, L2ChainId,
};
use zksync_web3_decl::{
    client::{DynClient, L1, L2},
    error::EnrichedClientError,
    namespaces::ZksNamespaceClient,
};

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum DataAvailabilityFetcherHealth {
    Ready {
        #[serde(skip_serializing_if = "Option::is_none")]
        last_dispatched_batch_number: Option<L1BatchNumber>,
    },
    Affected {
        error: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DataAvailabilityFetcherError {
    #[error("error fetching data from main node: {0}")]
    Rpc(#[from] anyhow::Error),
    #[error("error calling DA layer: {0}")]
    DALayer(#[from] anyhow::Error),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug)]
enum StepOutcome {
    UpdatedBatch(L1BatchNumber),
    NoProgress,
    NoInclusionDataFromMainNode,
    UnableToFetchInclusionData,
}
impl From<DataAvailabilityFetcherHealth> for Health {
    fn from(health: DataAvailabilityFetcherHealth) -> Self {
        let status = match health {
            DataAvailabilityFetcherHealth::Ready { .. } => HealthStatus::Ready,
            DataAvailabilityFetcherHealth::Affected { .. } => HealthStatus::Affected,
        };
        Self::from(status).with_details(health)
    }
}

/// Component fetches the Data Availability info from the main node and persists this data in Postgres.
/// The persisted data will be checked against L1 commitment transactions by Consistency checker.
#[derive(Debug)]
pub struct DataAvailabilityFetcher {
    client: Box<DynClient<L2>>,
    da_client: Box<dyn DataAvailabilityClient>,
    pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    poll_interval: Duration,
}

impl DataAvailabilityFetcher {
    const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(1000);

    /// Creates a new fetcher connected to the main node.
    pub fn new(
        client: Box<DynClient<L2>>,
        pool: ConnectionPool<Core>,
        da_client: Box<dyn DataAvailabilityClient>,
    ) -> Self {
        Self {
            client: client.for_component("data_availability_fetcher"),
            da_client,
            pool,
            health_updater: ReactiveHealthCheck::new("data_availability_fetcher").1,
            poll_interval: Self::DEFAULT_POLL_INTERVAL,
        }
    }

    /// Returns a health check for this fetcher.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn get_batch_to_fetch(&self) -> anyhow::Result<Option<L1BatchNumber>> {
        let mut storage = self
            .pool
            .connection_tagged("data_availability_fetcher")
            .await?;
        // Fetch data in a readonly transaction to have a consistent view of the storage
        let mut storage = storage.start_transaction().await?;

        let last_l1_batch = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        let Some(last_l1_batch) = last_l1_batch else {
            tracing::debug!("No L1 batches in the database yet; cannot progress");
            return Ok(None);
        };

        let last_l1_batch_with_da_info = storage
            .data_availability_dal()
            .get_latest_batch_with_inclusion_data()
            .await?;

        let l1_batch_to_fetch = if let Some(batch) = last_l1_batch_with_da_info {
            batch + 1
        } else {
            let earliest_l1_batch = storage
                .blocks_dal()
                .get_earliest_l1_batch_number()
                .await?
                .context("all L1 batches disappeared from Postgres")?;

            tracing::debug!("No L1 batches with DA info present in the storage; will fetch the earliest batch #{earliest_l1_batch}");
            earliest_l1_batch
        };

        Ok(if l1_batch_to_fetch <= last_l1_batch {
            Some(l1_batch_to_fetch)
        } else {
            None
        })
    }

    async fn step(&mut self) -> Result<StepOutcome, DataAvailabilityFetcherError> {
        let l1_batch_to_fetch = self
            .get_batch_to_fetch()
            .await
            .map_err(|err| DataAvailabilityFetcherError::Internal(err))?;

        let Some(l1_batch_to_fetch) = l1_batch_to_fetch else {
            return Ok(StepOutcome::NoProgress);
        };

        tracing::debug!("Fetching DA info for L1 batch #{l1_batch_to_fetch}");
        let da_details = self
            .client
            .get_data_availability_details(l1_batch_to_fetch)
            .await
            .map_err(|err| DataAvailabilityFetcherError::Rpc(err.into()))?;

        if da_details.pubdata_type.to_string() != self.da_client.name() {
            return Err(DataAvailabilityFetcherError::Internal(anyhow::anyhow!(
                "DA client mismatch, used in config: {}, received from main node: {}",
                self.da_client.name(),
                da_details.pubdata_type
            )));
        }

        if da_details.inclusion_data.is_none() {
            return Ok(StepOutcome::NoInclusionDataFromMainNode);
        }

        let inclusion_data = self
            .da_client
            .get_inclusion_data(da_details.blob_id.as_str())
            .await
            .map_err(|err| {
                DataAvailabilityFetcherError::DALayer(anyhow::anyhow!(
                    "Error fetching inclusion data: {err}"
                ))
            })?;

        let Some(inclusion_data) = inclusion_data else {
            Ok(StepOutcome::UnableToFetchInclusionData)
        };

        let mut connection = self
            .pool
            .connection_tagged("data_availability_fetcher")
            .await
            .map_err(|err| DataAvailabilityFetcherError::Internal(err.generalize()))?;
        connection
            .data_availability_dal()
            .insert_l1_batch_da(
                l1_batch_to_fetch,
                da_details.blob_id.as_str(),
                da_details.sent_at.naive_utc(),
                da_details.pubdata_type,
                Some(inclusion_data.data.as_slice()),
            )
            .await
            .map_err(|err| DataAvailabilityFetcherError::Internal(err.generalize()))?;

        tracing::debug!(
            "Updated L1 batch #{} with DA blob id: {}",
            l1_batch_to_fetch,
            da_details.blob_id
        );
        Ok(StepOutcome::UpdatedBatch(l1_batch_to_fetch))
    }

    fn update_health(&self, last_dispatched_batch_number: Option<L1BatchNumber>) {
        let health = DataAvailabilityFetcherHealth::Ready {
            last_dispatched_batch_number,
        };
        self.health_updater.update(health.into());
    }

    /// Runs this component until a fatal error occurs or a stop signal is received. Retriable errors
    /// (e.g., no network connection) are handled gracefully by retrying after a delay.
    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));
        let mut last_updated_l1_batch = None;

        while !*stop_receiver.borrow_and_update() {
            let step_outcome = self.step().await;
            let need_to_sleep = match step_outcome {
                Ok(StepOutcome::UpdatedBatch(batch_number)) => {
                    last_updated_l1_batch = Some(batch_number);
                    self.update_health(last_updated_l1_batch);
                    false
                }
                Ok(StepOutcome::NoProgress) | Ok(StepOutcome::NoInclusionDataFromMainNode) => {
                    // Update health status even if no progress was made to timely clear a previously set
                    // "affected" health.
                    self.update_health(last_updated_l1_batch);
                    true
                }
                Ok(StepOutcome::UnableToFetchInclusionData) => {
                    tracing::warn!(
                        "No inclusion data for the batch from DA layer, will retry later"
                    );
                    self.update_health(last_updated_l1_batch);
                    true
                }
                Err(err) => {
                    match err {
                        DataAvailabilityFetcherError::Rpc(e) => {}
                        DataAvailabilityFetcherError::DALayer(e) => {}
                        DataAvailabilityFetcherError::Internal(e) => {}
                    }
                    tracing::warn!(
                        "Transient error in tree data fetcher, will retry after a delay: {err:?}"
                    );
                    let health = DataAvailabilityFetcherHealth::Affected {
                        error: err.to_string(),
                    };
                    self.health_updater.update(health.into());
                    true
                }
            };

            if need_to_sleep
                && tokio::time::timeout(self.poll_interval, stop_receiver.changed())
                    .await
                    .is_ok()
            {
                break;
            }
        }
        tracing::info!("Stop signal received; tree data fetcher is shutting down");
        Ok(())
    }
}
