//! Fetcher is responsible for getting DA information from the main node.

use std::time::Duration;

use anyhow::Context as _;
use serde::Serialize;
use tokio::sync::watch;
use zksync_da_client::{types::InclusionData, DataAvailabilityClient};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{commitment::PubdataType, utils::client_type_to_pubdata_type, L1BatchNumber};
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::UnstableNamespaceClient,
};

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum DataAvailabilityFetcherHealth {
    Ready {
        #[serde(skip_serializing_if = "Option::is_none")]
        last_fetched_batch_number: Option<L1BatchNumber>,
    },
    Affected {
        error: String,
    },
}

#[derive(Debug)]
struct DataAvailabilityFetcherError {
    error: anyhow::Error,
    is_retriable: bool,
}

fn to_retriable_error(err: anyhow::Error) -> DataAvailabilityFetcherError {
    DataAvailabilityFetcherError {
        error: err,
        is_retriable: true,
    }
}

fn to_fatal_error(err: anyhow::Error) -> DataAvailabilityFetcherError {
    DataAvailabilityFetcherError {
        error: err,
        is_retriable: false,
    }
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
    const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(2000);

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
                .context("all L1 batches disappeared from Postgres")?
                .max(L1BatchNumber(1)); // if only the genesis batch is in the storage, we should start from the first one, skipping the genesis

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
        let l1_batch_to_fetch = self.get_batch_to_fetch().await.map_err(to_fatal_error)?;

        let Some(l1_batch_to_fetch) = l1_batch_to_fetch else {
            return Ok(StepOutcome::NoProgress);
        };

        tracing::debug!("Fetching DA info for L1 batch #{l1_batch_to_fetch}");
        let Some(da_details) = self
            .client
            .get_data_availability_details(l1_batch_to_fetch)
            .await
            .map_err(|err| to_retriable_error(err.into()))?
        else {
            return Ok(StepOutcome::NoProgress);
        };

        let Some(pubdata_type) = da_details.pubdata_type else {
            tracing::warn!(
                "No pubdata type for L1 batch #{}; waiting for the main node to provide it",
                l1_batch_to_fetch
            );
            return Ok(StepOutcome::NoProgress);
        };

        let config_pubdata_type = client_type_to_pubdata_type(self.da_client.client_type());
        // if pubdata type of the DA client is NoDA and pubdata type of the EN is not - it means
        // that the main node is planning to use the DA layer, so ENs were configured earlier
        if pubdata_type != config_pubdata_type && pubdata_type != PubdataType::NoDA {
            return Err(to_fatal_error(anyhow::anyhow!(
                "DA client mismatch, used in config: {}, received from main node: {}",
                config_pubdata_type,
                pubdata_type
            )));
        }

        let Some(expected_inclusion_data) = da_details.inclusion_data else {
            return Ok(StepOutcome::NoInclusionDataFromMainNode);
        };

        let inclusion_data = match pubdata_type {
            PubdataType::NoDA => InclusionData::default(), // to handle Stage 0 -> Stage 1 Validium migration
            _ => {
                let inclusion_data_from_rpc = self
                    .da_client
                    .get_inclusion_data(da_details.blob_id.as_str())
                    .await
                    .map_err(|err| {
                        to_retriable_error(anyhow::anyhow!("Error fetching inclusion data: {err}"))
                    })?;

                match inclusion_data_from_rpc {
                    Some(data) => data,
                    None => return Ok(StepOutcome::UnableToFetchInclusionData),
                }
            }
        };

        // - if inclusion data is `Some`, but empty - it means that the main node uses dummy inclusion proofs
        //   it is a valid case, and we don't need to check that it matches the one from DA layer
        //   (this is especially important for the process of migration from Stage 1 to Stage 2 Validium)
        //
        // - if inclusion data is `Some` and not empty - it has to match the one retrieved from the DA layer
        if !expected_inclusion_data.is_empty() && expected_inclusion_data != inclusion_data.data {
            return Err(to_fatal_error(anyhow::anyhow!(
                "Inclusion data mismatch for DA blob id: {}; expected: {:?}, got: {:?}",
                da_details.blob_id,
                expected_inclusion_data,
                inclusion_data.data
            )));
        }

        let mut connection = self
            .pool
            .connection_tagged("data_availability_fetcher")
            .await
            .map_err(|err| to_fatal_error(err.generalize()))?;
        connection
            .data_availability_dal()
            .insert_l1_batch_da(
                l1_batch_to_fetch,
                da_details.blob_id.as_str(),
                da_details.sent_at.naive_utc(),
                pubdata_type,
                Some(inclusion_data.data.as_slice()),
                da_details.l2_da_validator,
            )
            .await
            .map_err(|err| to_fatal_error(err.generalize()))?;

        tracing::debug!(
            "Updated L1 batch #{} with DA blob id: {}",
            l1_batch_to_fetch,
            da_details.blob_id
        );
        Ok(StepOutcome::UpdatedBatch(l1_batch_to_fetch))
    }

    fn update_health(&self, last_fetched_batch_number: Option<L1BatchNumber>) {
        let health = DataAvailabilityFetcherHealth::Ready {
            last_fetched_batch_number,
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
                    if err.is_retriable {
                        tracing::warn!(
                            "Error in data availability fetcher, will retry after a delay: {err:?}"
                        );
                        let health = DataAvailabilityFetcherHealth::Affected {
                            error: err.error.to_string(),
                        };
                        self.health_updater.update(health.into());
                        true
                    } else {
                        tracing::error!("Fatal error in data availability fetcher: {err:?}");
                        return Err(err.error);
                    }
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
        tracing::info!("Stop signal received; data availability fetcher is shutting down");
        Ok(())
    }
}
