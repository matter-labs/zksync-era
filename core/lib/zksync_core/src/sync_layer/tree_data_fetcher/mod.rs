//! Fetcher responsible for getting Merkle tree outputs from the main node.

use std::{fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{api, block::L1BatchTreeData, L1BatchNumber};
use zksync_web3_decl::{
    client::BoxedL2Client,
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    namespaces::ZksNamespaceClient,
};

#[async_trait]
pub trait MainNodeClient: fmt::Debug + Send + Sync {
    async fn batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>>;
}

#[async_trait]
impl MainNodeClient for BoxedL2Client {
    async fn batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>> {
        self.get_l1_batch_details(number)
            .rpc_context("get_l1_batch_details")
            .with_arg("number", &number)
            .await
    }
}

#[derive(Debug, thiserror::Error)]
enum TreeDataFetcherError {
    #[error("error fetching data from main node")]
    Rpc(#[from] EnrichedClientError),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<DalError> for TreeDataFetcherError {
    fn from(err: DalError) -> Self {
        Self::Internal(err.generalize())
    }
}

impl TreeDataFetcherError {
    fn is_transient(&self) -> bool {
        match self {
            Self::Rpc(err) => err.is_transient(),
            Self::Internal(_) => false,
        }
    }
}

#[derive(Debug)]
enum StepOutcome {
    UpdatedBatch(L1BatchNumber),
    NoProgress,
    RemoteHashMissing,
}

#[derive(Debug)]
pub struct TreeDataFetcher {
    main_node_client: Box<dyn MainNodeClient>,
    pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    sleep_interval: Duration,
}

impl TreeDataFetcher {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(client: BoxedL2Client, pool: ConnectionPool<Core>) -> Self {
        Self {
            main_node_client: Box::new(client),
            pool,
            health_updater: ReactiveHealthCheck::new("tree_data_fetcher").1,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
        }
    }

    async fn get_batch_to_fetch(&self) -> anyhow::Result<Option<L1BatchNumber>> {
        let mut storage = self.pool.connection_tagged("tree_data_fetcher").await?;
        // Fetch data in a readonly transaction to have a consistent view of the storage
        let mut storage = storage.start_transaction().await?;

        let last_l1_batch = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        let Some(last_l1_batch) = last_l1_batch else {
            tracing::debug!("No L1 batches in the database yet; cannot progress");
            return Ok(None);
        };

        let last_l1_batch_with_tree_data = storage
            .blocks_dal()
            .get_last_l1_batch_number_with_tree_data()
            .await?;
        let l1_batch_to_fetch = if let Some(batch) = last_l1_batch_with_tree_data {
            batch + 1
        } else {
            let earliest_l1_batch = storage.blocks_dal().get_earliest_l1_batch_number().await?;
            let earliest_l1_batch =
                earliest_l1_batch.context("all L1 batches disappeared from Postgres")?;
            tracing::debug!("No L1 batches with metadata present in the storage; will fetch the earliest batch #{earliest_l1_batch}");
            earliest_l1_batch
        };
        Ok(if l1_batch_to_fetch <= last_l1_batch {
            Some(l1_batch_to_fetch)
        } else {
            None
        })
    }

    async fn get_rollup_last_leaf_index(
        storage: &mut Connection<'_, Core>,
        mut l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<u64> {
        // With overwhelming probability, there's at least one initial write in an L1 batch,
        // so this loop will execute for 1 iteration.
        loop {
            let maybe_index = storage
                .storage_logs_dedup_dal()
                .max_enumeration_index_for_l1_batch(l1_batch_number)
                .await?;
            if let Some(index) = maybe_index {
                return Ok(index + 1);
            }
            tracing::warn!(
                "No initial writes in L1 batch #{l1_batch_number}; trying the previous batch"
            );
            l1_batch_number -= 1;
        }
    }

    async fn step(&self) -> Result<StepOutcome, TreeDataFetcherError> {
        let Some(l1_batch_to_fetch) = self.get_batch_to_fetch().await? else {
            return Ok(StepOutcome::NoProgress);
        };
        tracing::debug!("Fetching tree data for L1 batch #{l1_batch_to_fetch} from main node");
        let batch_details = self
            .main_node_client
            .batch_details(l1_batch_to_fetch)
            .await?
            .with_context(|| {
                format!(
                    "L1 batch #{l1_batch_to_fetch} is sealed locally, but is not present on the main node, \
                     which is assumed to store batch info indefinitely"
                )
            })?;
        let Some(root_hash) = batch_details.base.root_hash else {
            tracing::debug!(
                "L1 batch #{l1_batch_to_fetch} does not have root hash computed on the main node"
            );
            return Ok(StepOutcome::RemoteHashMissing);
        };

        let mut storage = self.pool.connection_tagged("tree_data_fetcher").await?;
        let rollup_last_leaf_index =
            Self::get_rollup_last_leaf_index(&mut storage, l1_batch_to_fetch).await?;
        let tree_data = L1BatchTreeData {
            hash: root_hash,
            rollup_last_leaf_index,
        };
        storage
            .blocks_dal()
            .save_l1_batch_tree_data(l1_batch_to_fetch, &tree_data)
            .await?;
        Ok(StepOutcome::UpdatedBatch(l1_batch_to_fetch))
    }

    fn update_health(&self, last_updated_l1_batch: Option<L1BatchNumber>) {
        let health = Health::from(HealthStatus::Ready).with_details(serde_json::json!({
            "last_updated_l1_batch": last_updated_l1_batch,
        }));
        self.health_updater.update(health);
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));
        let mut last_updated_l1_batch = None;

        while !*stop_receiver.borrow_and_update() {
            let need_to_sleep = match self.step().await {
                Ok(StepOutcome::UpdatedBatch(batch_number)) => {
                    last_updated_l1_batch = Some(batch_number);
                    self.update_health(last_updated_l1_batch);
                    false
                }
                Ok(StepOutcome::NoProgress | StepOutcome::RemoteHashMissing) => {
                    // Update health status even if no progress was made to timely clear a previously set
                    // "affected" health.
                    self.update_health(last_updated_l1_batch);
                    true
                }
                Err(err) if err.is_transient() => {
                    tracing::warn!(
                        "Transient error in tree data fetcher, will retry after a delay: {err:?}"
                    );
                    let health =
                        Health::from(HealthStatus::Affected).with_details(serde_json::json!({
                            "error": err.to_string(),
                        }));
                    self.health_updater.update(health);
                    true
                }
                Err(err) => {
                    tracing::error!("Fatal error in tree data fetcher: {err:?}");
                    return Err(err.into());
                }
            };

            if need_to_sleep
                && tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
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
