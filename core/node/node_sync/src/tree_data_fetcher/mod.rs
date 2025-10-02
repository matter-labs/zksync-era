//! Fetcher responsible for getting Merkle tree outputs from the main node.

use std::time::Duration;

use anyhow::Context as _;
use serde::Serialize;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_eth_client::EthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    block::{L1BatchTreeData, L2BlockHeader},
    Address, L1BatchNumber,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::EnrichedClientError,
};

use self::{
    metrics::{ProcessingStage, TreeDataFetcherMetrics, METRICS},
    provider::{MissingData, SLDataProvider, TreeDataProvider},
};
use crate::tree_data_fetcher::provider::CombinedDataProvider;

mod metrics;
mod provider;
#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TreeDataFetcherError {
    #[error("error fetching data")]
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
    fn is_retriable(&self) -> bool {
        match self {
            Self::Rpc(err) => err.is_retryable(),
            Self::Internal(_) => false,
        }
    }
}

type TreeDataFetcherResult<T> = Result<T, TreeDataFetcherError>;

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum TreeDataFetcherHealth {
    Ready {
        #[serde(skip_serializing_if = "Option::is_none")]
        last_updated_l1_batch: Option<L1BatchNumber>,
    },
    Affected {
        error: String,
    },
}

impl From<TreeDataFetcherHealth> for Health {
    fn from(health: TreeDataFetcherHealth) -> Self {
        let status = match health {
            TreeDataFetcherHealth::Ready { .. } => HealthStatus::Ready,
            TreeDataFetcherHealth::Affected { .. } => HealthStatus::Affected,
        };
        Self::from(status).with_details(health)
    }
}

#[derive(Debug)]
enum StepOutcome {
    UpdatedBatch(L1BatchNumber),
    NoProgress,
    RemoteHashMissing,
    PossibleReorg,
}

/// Component fetching tree data (i.e., state root hashes for L1 batches) from external sources, such as
/// the main node, and persisting this data to Postgres.
///
/// # Overview
///
/// This component allows a node to operate w/o a Merkle tree or w/o waiting the tree to catch up.
/// It can be operated together with Metadata calculator or instead of it. In the first case, Metadata calculator
/// (which is generally expected to be slower) will check that data returned by the main node is correct
/// (i.e., "trust but verify" trust model). Additionally, the persisted data will be checked against L1 commitment transactions
/// by Consistency checker.
#[derive(Debug)]
pub struct TreeDataFetcher {
    data_provider: CombinedDataProvider,
    // Used in the Info metric
    diamond_proxy_address: Option<Address>,
    pool: ConnectionPool<Core>,
    metrics: &'static TreeDataFetcherMetrics,
    health_updater: HealthUpdater,
    poll_interval: Duration,
    #[cfg(test)]
    updates_sender: mpsc::UnboundedSender<L1BatchNumber>,
}

impl TreeDataFetcher {
    const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

    /// Creates a new fetcher connected to the main node.
    pub fn new(client: Box<DynClient<L2>>, pool: ConnectionPool<Core>) -> Self {
        Self {
            data_provider: CombinedDataProvider::new(client.for_component("tree_data_fetcher")),
            diamond_proxy_address: None,
            pool,
            metrics: &METRICS,
            health_updater: ReactiveHealthCheck::new("tree_data_fetcher").1,
            poll_interval: Self::DEFAULT_POLL_INTERVAL,
            #[cfg(test)]
            updates_sender: mpsc::unbounded_channel().0,
        }
    }

    /// Attempts to fetch root hashes from L1 (namely, `BlockCommit` events emitted by the diamond proxy) if possible.
    /// The main node will still be used as a fallback in case communicating with L1 fails, or for newer batches,
    /// which may not be committed on L1.
    pub async fn with_sl_data(
        mut self,
        sl_client: Box<dyn EthInterface>,
        sl_diamond_proxy_addr: Address,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.diamond_proxy_address.is_none(),
            "L1 tree data provider is already set up"
        );

        let sl_provider = SLDataProvider::new(sl_client, sl_diamond_proxy_addr).await?;
        self.data_provider.set_sl(sl_provider);
        self.diamond_proxy_address = Some(sl_diamond_proxy_addr);
        Ok(self)
    }

    /// Returns a health check for this fetcher.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn get_batch_to_fetch(&self) -> anyhow::Result<Option<(L1BatchNumber, L2BlockHeader)>> {
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
            let last_l2_block = Self::get_last_l2_block(&mut storage, l1_batch_to_fetch).await?;
            Some((l1_batch_to_fetch, last_l2_block))
        } else {
            None
        })
    }

    async fn get_last_l2_block(
        storage: &mut Connection<'_, Core>,
        number: L1BatchNumber,
    ) -> anyhow::Result<L2BlockHeader> {
        let (_, last_l2_block_number) = storage
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(number)
            .await?
            .with_context(|| format!("L1 batch #{number} disappeared from Postgres"))?;
        storage
            .blocks_dal()
            .get_l2_block_header(last_l2_block_number)
            .await?
            .with_context(|| format!("L2 block #{last_l2_block_number} (last for L1 batch #{number}) disappeared from Postgres"))
    }

    async fn step(&mut self) -> Result<StepOutcome, TreeDataFetcherError> {
        let Some((l1_batch_to_fetch, last_l2_block_header)) = self.get_batch_to_fetch().await?
        else {
            return Ok(StepOutcome::NoProgress);
        };

        tracing::debug!("Fetching tree data for L1 batch #{l1_batch_to_fetch}");
        let stage_latency = self.metrics.stage_latency[&ProcessingStage::Fetch].start();
        let root_hash_result = self
            .data_provider
            .batch_details(l1_batch_to_fetch, &last_l2_block_header)
            .await?;
        stage_latency.observe();
        let root_hash = match root_hash_result {
            Ok(root_hash) => {
                tracing::debug!(
                    "Received root hash for L1 batch #{l1_batch_to_fetch}: {root_hash:?}"
                );
                root_hash
            }
            Err(MissingData::Batch) => {
                let err = anyhow::anyhow!(
                    "L1 batch #{l1_batch_to_fetch} is sealed locally, but is not present externally, \
                     which is assumed to store batch info indefinitely"
                );
                return Err(err.into());
            }
            Err(MissingData::RootHash) => {
                tracing::debug!(
                    "L1 batch #{l1_batch_to_fetch} does not have root hash computed externally"
                );
                return Ok(StepOutcome::RemoteHashMissing);
            }
            Err(MissingData::PossibleReorg) => {
                tracing::debug!(
                    "L1 batch #{l1_batch_to_fetch} potentially diverges from the external source"
                );
                return Ok(StepOutcome::PossibleReorg);
            }
        };

        let stage_latency = self.metrics.stage_latency[&ProcessingStage::Persistence].start();
        let mut storage = self.pool.connection_tagged("tree_data_fetcher").await?;
        let rollup_last_leaf_index = storage
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(l1_batch_to_fetch)
            .await?
            .unwrap_or(0)
            + 1;
        let tree_data = L1BatchTreeData {
            hash: root_hash,
            rollup_last_leaf_index,
        };
        storage
            .blocks_dal()
            .save_l1_batch_tree_data(l1_batch_to_fetch, &tree_data)
            .await?;
        stage_latency.observe();
        tracing::debug!("Updated L1 batch #{l1_batch_to_fetch} with tree data: {tree_data:?}");
        Ok(StepOutcome::UpdatedBatch(l1_batch_to_fetch))
    }

    fn update_health(&self, last_updated_l1_batch: Option<L1BatchNumber>) {
        let health = TreeDataFetcherHealth::Ready {
            last_updated_l1_batch,
        };
        self.health_updater.update(health.into());
    }

    /// Runs this component until a fatal error occurs or a stop request is received. Retriable errors
    /// (e.g., no network connection) are handled gracefully by retrying after a delay.
    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.metrics.observe_info(&self);
        self.health_updater
            .update(Health::from(HealthStatus::Ready));
        let mut last_updated_l1_batch = self
            .pool
            .connection_tagged("tree_data_fetcher")
            .await?
            .blocks_dal()
            .get_last_l1_batch_number_with_tree_data()
            .await?;

        while !*stop_receiver.borrow_and_update() {
            let step_outcome = self.step().await;
            self.metrics.observe_step_outcome(step_outcome.as_ref());
            let need_to_sleep = match step_outcome {
                Ok(StepOutcome::UpdatedBatch(batch_number)) => {
                    #[cfg(test)]
                    self.updates_sender.send(batch_number).ok();

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
                Ok(StepOutcome::PossibleReorg) => {
                    tracing::info!("Potential chain reorg detected by tree data fetcher; not updating tree data");
                    // Since we don't trust the reorg logic in the tree data fetcher, we let it continue working
                    // so that, if there's a false positive, the whole node doesn't crash (or is in a crash loop in the worst-case scenario).
                    let health = TreeDataFetcherHealth::Affected {
                        error: "Potential chain reorg".to_string(),
                    };
                    self.health_updater.update(health.into());
                    true
                }
                Err(err) if err.is_retriable() => {
                    tracing::warn!(
                        "Transient error in tree data fetcher, will retry after a delay: {err:?}"
                    );
                    let health = TreeDataFetcherHealth::Affected {
                        error: err.to_string(),
                    };
                    self.health_updater.update(health.into());
                    true
                }
                Err(err) => {
                    tracing::error!("Fatal error in tree data fetcher: {err:?}");
                    return Err(err.into());
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
        tracing::info!("Stop request received; tree data fetcher is shutting down");
        Ok(())
    }
}
