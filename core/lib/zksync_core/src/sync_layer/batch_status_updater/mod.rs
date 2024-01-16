//! Component responsible for updating L1 batch status.

use std::{fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{
    aggregated_operations::AggregatedActionType, api, L1BatchNumber, MiniblockNumber, H256,
};
use zksync_web3_decl::{
    jsonrpsee::{
        core::ClientError,
        http_client::{HttpClient, HttpClientBuilder},
    },
    namespaces::ZksNamespaceClient,
};

use super::metrics::{FetchStage, FETCHER_METRICS};
use crate::{metrics::EN_METRICS, utils::projected_first_l1_batch};

#[cfg(test)]
mod tests;

fn l1_batch_stage_to_action_str(stage: AggregatedActionType) -> &'static str {
    match stage {
        AggregatedActionType::Commit => "committed",
        AggregatedActionType::PublishProofOnchain => "proven",
        AggregatedActionType::Execute => "executed",
    }
}

/// Represents a change in the batch status.
/// It may be a batch being committed, proven or executed.
#[derive(Debug)]
struct BatchStatusChange {
    number: L1BatchNumber,
    l1_tx_hash: H256,
    happened_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
struct StatusChanges {
    commit: Vec<BatchStatusChange>,
    prove: Vec<BatchStatusChange>,
    execute: Vec<BatchStatusChange>,
}

impl StatusChanges {
    /// Returns true if there are no status changes.
    fn is_empty(&self) -> bool {
        self.commit.is_empty() && self.prove.is_empty() && self.execute.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
enum UpdaterError {
    #[error("JSON-RPC error communicating with main node")]
    Web3(#[from] ClientError),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::SqlxError> for UpdaterError {
    fn from(err: zksync_dal::SqlxError) -> Self {
        Self::Internal(err.into())
    }
}

#[async_trait]
trait MainNodeClient: fmt::Debug + Send + Sync {
    /// Returns any miniblock in the specified L1 batch.
    async fn resolve_l1_batch_to_miniblock(
        &self,
        number: L1BatchNumber,
    ) -> Result<Option<MiniblockNumber>, ClientError>;

    async fn block_details(
        &self,
        number: MiniblockNumber,
    ) -> Result<Option<api::BlockDetails>, ClientError>;
}

#[async_trait]
impl MainNodeClient for HttpClient {
    async fn resolve_l1_batch_to_miniblock(
        &self,
        number: L1BatchNumber,
    ) -> Result<Option<MiniblockNumber>, ClientError> {
        let request_latency = FETCHER_METRICS.requests[&FetchStage::GetMiniblockRange].start();
        let number = self
            .get_miniblock_range(number)
            .await?
            .map(|(start, _)| MiniblockNumber(start.as_u32()));
        request_latency.observe();
        Ok(number)
    }

    async fn block_details(
        &self,
        number: MiniblockNumber,
    ) -> Result<Option<api::BlockDetails>, ClientError> {
        let request_latency = FETCHER_METRICS.requests[&FetchStage::GetBlockDetails].start();
        let details = self.get_block_details(number).await?;
        request_latency.observe();
        Ok(details)
    }
}

/// Cursors for the last executed / proven / committed L1 batch numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
struct UpdaterCursor {
    last_executed_l1_batch: L1BatchNumber,
    last_proven_l1_batch: L1BatchNumber,
    last_committed_l1_batch: L1BatchNumber,
}

impl UpdaterCursor {
    async fn new(storage: &mut StorageProcessor<'_>) -> anyhow::Result<Self> {
        let first_l1_batch_number = projected_first_l1_batch(storage).await?;
        // Use the snapshot L1 batch, or the genesis batch if we are not using a snapshot. Technically, the snapshot L1 batch
        // is not necessarily proven / executed yet, but since it and earlier batches are not stored, it serves
        // a natural lower boundary for the cursor.
        let starting_l1_batch_number = L1BatchNumber(first_l1_batch_number.saturating_sub(1));

        let last_executed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_executed_on_eth()
            .await?
            .unwrap_or(starting_l1_batch_number);
        let last_proven_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_proven_on_eth()
            .await?
            .unwrap_or(starting_l1_batch_number);
        let last_committed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await?
            .unwrap_or(starting_l1_batch_number);
        Ok(Self {
            last_executed_l1_batch,
            last_proven_l1_batch,
            last_committed_l1_batch,
        })
    }

    fn extract_tx_hash_and_timestamp(
        batch_info: &api::BlockDetails,
        stage: AggregatedActionType,
    ) -> (Option<H256>, Option<DateTime<Utc>>) {
        match stage {
            AggregatedActionType::Commit => {
                (batch_info.base.commit_tx_hash, batch_info.base.committed_at)
            }
            AggregatedActionType::PublishProofOnchain => {
                (batch_info.base.prove_tx_hash, batch_info.base.proven_at)
            }
            AggregatedActionType::Execute => {
                (batch_info.base.execute_tx_hash, batch_info.base.executed_at)
            }
        }
    }

    fn update(
        &mut self,
        status_changes: &mut StatusChanges,
        batch_info: &api::BlockDetails,
    ) -> anyhow::Result<()> {
        for stage in [
            AggregatedActionType::Commit,
            AggregatedActionType::PublishProofOnchain,
            AggregatedActionType::Execute,
        ] {
            self.update_stage(status_changes, batch_info, stage)?;
        }
        Ok(())
    }

    fn update_stage(
        &mut self,
        status_changes: &mut StatusChanges,
        batch_info: &api::BlockDetails,
        stage: AggregatedActionType,
    ) -> anyhow::Result<()> {
        let (l1_tx_hash, happened_at) = Self::extract_tx_hash_and_timestamp(batch_info, stage);
        let (last_l1_batch, changes_to_update) = match stage {
            AggregatedActionType::Commit => (
                &mut self.last_committed_l1_batch,
                &mut status_changes.commit,
            ),
            AggregatedActionType::PublishProofOnchain => {
                (&mut self.last_proven_l1_batch, &mut status_changes.prove)
            }
            AggregatedActionType::Execute => (
                &mut self.last_executed_l1_batch,
                &mut status_changes.execute,
            ),
        };

        // Check whether we have all data for the update.
        let Some(l1_tx_hash) = l1_tx_hash else {
            return Ok(());
        };
        if batch_info.l1_batch_number != last_l1_batch.next() {
            return Ok(());
        }

        let action_str = l1_batch_stage_to_action_str(stage);
        let happened_at = happened_at.with_context(|| {
            format!("Malformed API response: batch is {action_str}, but has no relevant timestamp")
        })?;
        changes_to_update.push(BatchStatusChange {
            number: batch_info.l1_batch_number,
            l1_tx_hash,
            happened_at,
        });
        tracing::info!("Batch {}: {action_str}", batch_info.l1_batch_number);
        FETCHER_METRICS.l1_batch[&stage.into()].set(batch_info.l1_batch_number.0.into());
        *last_l1_batch += 1;
        Ok(())
    }
}

/// Component responsible for fetching the batch status changes, i.e. one that monitors whether the
/// locally applied batch was committed, proven or executed on L1.
///
/// In essence, it keeps track of the last batch number per status, and periodically polls the main
/// node on these batches in order to see whether the status has changed. If some changes were picked up,
/// the module updates the database to mirror the state observable from the main node. This is required for other components
/// (e.g., the API server and the consistency checker) to function properly. E.g., the API server returns commit / prove / execute
/// L1 transaction information in `zks_getBlockDetails` and `zks_getL1BatchDetails` RPC methods.
#[derive(Debug)]
pub struct BatchStatusUpdater {
    client: Box<dyn MainNodeClient>,
    pool: ConnectionPool,
    sleep_interval: Duration,
    /// Test-only sender of status changes each time they are produced and applied to the storage.
    #[cfg(test)]
    changes_sender: mpsc::UnboundedSender<StatusChanges>,
}

impl BatchStatusUpdater {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(main_node_url: &str, pool: ConnectionPool) -> anyhow::Result<Self> {
        let client = HttpClientBuilder::default()
            .build(main_node_url)
            .context("Unable to create a main node client")?;
        Ok(Self::from_parts(
            Box::new(client),
            pool,
            Self::DEFAULT_SLEEP_INTERVAL,
        ))
    }

    fn from_parts(
        client: Box<dyn MainNodeClient>,
        pool: ConnectionPool,
        sleep_interval: Duration,
    ) -> Self {
        Self {
            client,
            pool,
            sleep_interval,
            #[cfg(test)]
            changes_sender: mpsc::unbounded_channel().0,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut storage = self.pool.access_storage_tagged("sync_layer").await?;
        let mut cursor = UpdaterCursor::new(&mut storage).await?;
        drop(storage);
        tracing::info!("Initialized batch status updater cursor: {cursor:?}");

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, exiting the batch status updater routine");
                return Ok(());
            }

            // Status changes are created externally, so that even if we will receive a network error
            // while requesting the changes, we will be able to process what we already fetched.
            let mut status_changes = StatusChanges::default();
            // Note that we don't update `cursor` here (it is copied), but rather only in `apply_status_changes`.
            match self.get_status_changes(&mut status_changes, cursor).await {
                Ok(()) => { /* everything went smoothly */ }
                Err(UpdaterError::Web3(err)) => {
                    tracing::warn!("Failed to get status changes from the main node: {err}");
                }
                Err(UpdaterError::Internal(err)) => return Err(err),
            }

            if status_changes.is_empty() {
                tokio::time::sleep(self.sleep_interval).await;
            } else {
                self.apply_status_changes(&mut cursor, status_changes)
                    .await?;
            }
        }
    }

    /// Goes through the already fetched batches trying to update their statuses.
    ///
    /// Fetched changes are capped by the last locally applied batch number, so
    /// it's safe to assume that every status change can safely be applied (no status
    /// changes "from the future").
    async fn get_status_changes(
        &self,
        status_changes: &mut StatusChanges,
        mut cursor: UpdaterCursor,
    ) -> Result<(), UpdaterError> {
        let total_latency = EN_METRICS.update_batch_statuses.start();
        let Some(last_sealed_batch) = self
            .pool
            .access_storage_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
        else {
            return Ok(()); // No L1 batches in the storage yet; do nothing.
        };

        let mut batch = cursor.last_executed_l1_batch.next();
        // In this loop we try to progress on the batch statuses, utilizing the same request to the node to potentially
        // update all three statuses (e.g. if the node is still syncing), but also skipping the gaps in the statuses
        // (e.g. if the last executed batch is 10, but the last proven is 20, we don't need to check the batches 11-19).
        while batch <= last_sealed_batch {
            // While we may receive `None` for the `self.current_l1_batch`, it's OK: open batch is guaranteed to not
            // be sent to L1.
            let miniblock_number = self.client.resolve_l1_batch_to_miniblock(batch).await?;
            let Some(miniblock_number) = miniblock_number else {
                return Ok(());
            };

            let Some(batch_info) = self.client.block_details(miniblock_number).await? else {
                // We cannot recover from an external API inconsistency.
                let err = anyhow::anyhow!(
                    "Node API is inconsistent: miniblock {miniblock_number} was reported to be a part of {batch} L1 batch, \
                    but API has no information about this miniblock",
                );
                return Err(err.into());
            };

            cursor.update(status_changes, &batch_info)?;

            // Check whether we can skip a part of the range.
            if batch_info.base.commit_tx_hash.is_none() {
                // No committed batches after this one.
                break;
            } else if batch_info.base.prove_tx_hash.is_none()
                && batch < cursor.last_committed_l1_batch
            {
                // The interval between this batch and the last committed one is not proven.
                batch = cursor.last_committed_l1_batch.next();
            } else if batch_info.base.executed_at.is_none() && batch < cursor.last_proven_l1_batch {
                // The interval between this batch and the last proven one is not executed.
                batch = cursor.last_proven_l1_batch.next();
            } else {
                batch += 1;
            }
        }

        total_latency.observe();
        Ok(())
    }

    /// Inserts the provided status changes into the database.
    /// The status changes are applied to the database by inserting bogus confirmed transactions (with
    /// some fields missing/substituted) only to satisfy API needs; this component doesn't expect the updated
    /// tables to be ever accessed by the `eth_sender` module.
    async fn apply_status_changes(
        &self,
        cursor: &mut UpdaterCursor,
        changes: StatusChanges,
    ) -> anyhow::Result<()> {
        let total_latency = EN_METRICS.batch_status_updater_loop_iteration.start();
        let mut connection = self.pool.access_storage_tagged("sync_layer").await?;
        let mut transaction = connection.start_transaction().await?;
        let last_sealed_batch = transaction
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
            .context("L1 batches disappeared from Postgres")?;

        for change in &changes.commit {
            tracing::info!(
                "Commit status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
            anyhow::ensure!(
                change.number <= last_sealed_batch,
                "Incorrect update state: unknown batch marked as committed"
            );

            transaction
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::Commit,
                    change.l1_tx_hash,
                    change.happened_at,
                )
                .await?;
            cursor.last_committed_l1_batch = change.number;
        }

        for change in &changes.prove {
            tracing::info!(
                "Prove status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
            anyhow::ensure!(
                change.number <= cursor.last_committed_l1_batch,
                "Incorrect update state: proven batch must be committed"
            );

            transaction
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::PublishProofOnchain,
                    change.l1_tx_hash,
                    change.happened_at,
                )
                .await?;
            cursor.last_proven_l1_batch = change.number;
        }

        for change in &changes.execute {
            tracing::info!(
                "Execute status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
            anyhow::ensure!(
                change.number <= cursor.last_proven_l1_batch,
                "Incorrect update state: executed batch must be proven"
            );

            transaction
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::Execute,
                    change.l1_tx_hash,
                    change.happened_at,
                )
                .await?;
            cursor.last_executed_l1_batch = change.number;
        }
        transaction.commit().await?;
        total_latency.observe();

        #[cfg(test)]
        self.changes_sender.send(changes).ok();
        Ok(())
    }
}
