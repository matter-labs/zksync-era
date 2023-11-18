use chrono::{DateTime, Utc};
use tokio::sync::watch::Receiver;

use std::time::Duration;

use zksync_dal::ConnectionPool;
use zksync_types::{
    aggregated_operations::AggregatedActionType, api::BlockDetails, L1BatchNumber, MiniblockNumber,
    H256,
};
use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::ZksNamespaceClient,
    RpcResult,
};

use super::metrics::{FetchStage, L1BatchStage, FETCHER_METRICS};
use crate::metrics::EN_METRICS;

/// Represents a change in the batch status.
/// It may be a batch being committed, proven or executed.
#[derive(Debug)]
pub(crate) struct BatchStatusChange {
    pub(crate) number: L1BatchNumber,
    pub(crate) l1_tx_hash: H256,
    pub(crate) happened_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
struct StatusChanges {
    commit: Vec<BatchStatusChange>,
    prove: Vec<BatchStatusChange>,
    execute: Vec<BatchStatusChange>,
}

impl StatusChanges {
    fn new() -> Self {
        Self::default()
    }

    /// Returns true if there are no status changes.
    fn is_empty(&self) -> bool {
        self.commit.is_empty() && self.prove.is_empty() && self.execute.is_empty()
    }
}

/// Module responsible for fetching the batch status changes, i.e. one that monitors whether the
/// locally applied batch was committed, proven or executed on L1.
///
/// In essence, it keeps track of the last batch number per status, and periodically polls the main
/// node on these batches in order to see whether the status has changed. If some changes were picked up,
/// the module updates the database to mirror the state observable from the main node.
#[derive(Debug)]
pub struct BatchStatusUpdater {
    client: HttpClient,
    pool: ConnectionPool,

    last_executed_l1_batch: L1BatchNumber,
    last_proven_l1_batch: L1BatchNumber,
    last_committed_l1_batch: L1BatchNumber,
}

impl BatchStatusUpdater {
    pub async fn new(main_node_url: &str, pool: ConnectionPool) -> Self {
        let client = HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Unable to create a main node client");

        let mut storage = pool.access_storage_tagged("sync_layer").await.unwrap();
        let last_executed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_executed_on_eth()
            .await
            .unwrap()
            .unwrap_or_default();
        let last_proven_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_proven_on_eth()
            .await
            .unwrap()
            .unwrap_or_default();
        let last_committed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await
            .unwrap()
            .unwrap_or_default();
        drop(storage);

        Self {
            client,
            pool,

            last_committed_l1_batch,
            last_proven_l1_batch,
            last_executed_l1_batch,
        }
    }

    pub async fn run(mut self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, exiting the batch status updater routine");
                return Ok(());
            }
            // Status changes are created externally, so that even if we will receive a network error
            // while requesting the changes, we will be able to process what we already fetched.
            let mut status_changes = StatusChanges::new();
            if let Err(err) = self.get_status_changes(&mut status_changes).await {
                tracing::warn!("Failed to get status changes from the database: {err}");
            };

            if status_changes.is_empty() {
                const DELAY_INTERVAL: Duration = Duration::from_secs(5);
                tokio::time::sleep(DELAY_INTERVAL).await;
                continue;
            }

            self.apply_status_changes(status_changes).await;
        }
    }

    /// Goes through the already fetched batches trying to update their statuses.
    /// Returns a collection of the status updates grouped by the operation type.
    ///
    /// Fetched changes are capped by the last locally applied batch number, so
    /// it's safe to assume that every status change can safely be applied (no status
    /// changes "from the future").
    async fn get_status_changes(&self, status_changes: &mut StatusChanges) -> RpcResult<()> {
        let total_latency = EN_METRICS.update_batch_statuses.start();
        let last_sealed_batch = self
            .pool
            .access_storage_tagged("sync_layer")
            .await
            .unwrap()
            .blocks_dal()
            .get_newest_l1_batch_header()
            .await
            .unwrap()
            .number;

        // We don't want to change the internal state until we actually persist the changes.
        let mut last_committed_l1_batch = self.last_committed_l1_batch;
        let mut last_proven_l1_batch = self.last_proven_l1_batch;
        let mut last_executed_l1_batch = self.last_executed_l1_batch;

        assert!(
            last_executed_l1_batch <= last_proven_l1_batch,
            "Incorrect local state: executed batch must be proven"
        );
        assert!(
            last_proven_l1_batch <= last_committed_l1_batch,
            "Incorrect local state: proven batch must be committed"
        );
        assert!(
            last_committed_l1_batch <= last_sealed_batch,
            "Incorrect local state: unkonwn batch marked as committed"
        );

        let mut batch = last_executed_l1_batch.next();
        // In this loop we try to progress on the batch statuses, utilizing the same request to the node to potentially
        // update all three statuses (e.g. if the node is still syncing), but also skipping the gaps in the statuses
        // (e.g. if the last executed batch is 10, but the last proven is 20, we don't need to check the batches 11-19).
        while batch <= last_sealed_batch {
            // While we may receive `None` for the `self.current_l1_batch`, it's OK: open batch is guaranteed to not
            // be sent to L1.
            let request_latency = FETCHER_METRICS.requests[&FetchStage::GetMiniblockRange].start();
            let Some((start_miniblock, _)) = self.client.get_miniblock_range(batch).await? else {
                return Ok(());
            };
            request_latency.observe();

            // We could've used any miniblock from the range, all of them share the same info.
            let request_latency = FETCHER_METRICS.requests[&FetchStage::GetBlockDetails].start();
            let Some(batch_info) = self
                .client
                .get_block_details(MiniblockNumber(start_miniblock.as_u32()))
                .await?
            else {
                // We cannot recover from an external API inconsistency.
                panic!(
                    "Node API is inconsistent: miniblock {} was reported to be a part of {} L1 batch, \
                    but API has no information about this miniblock", start_miniblock, batch
                );
            };
            request_latency.observe();

            Self::update_committed_batch(status_changes, &batch_info, &mut last_committed_l1_batch);
            Self::update_proven_batch(status_changes, &batch_info, &mut last_proven_l1_batch);
            Self::update_executed_batch(status_changes, &batch_info, &mut last_executed_l1_batch);

            // Check whether we can skip a part of the range.
            if batch_info.base.commit_tx_hash.is_none() {
                // No committed batches after this one.
                break;
            } else if batch_info.base.prove_tx_hash.is_none() && batch < last_committed_l1_batch {
                // The interval between this batch and the last committed one is not proven.
                batch = last_committed_l1_batch.next();
            } else if batch_info.base.executed_at.is_none() && batch < last_proven_l1_batch {
                // The interval between this batch and the last proven one is not executed.
                batch = last_proven_l1_batch.next();
            } else {
                batch += 1;
            }
        }

        total_latency.observe();
        Ok(())
    }

    fn update_committed_batch(
        status_changes: &mut StatusChanges,
        batch_info: &BlockDetails,
        last_committed_l1_batch: &mut L1BatchNumber,
    ) {
        if batch_info.base.commit_tx_hash.is_some()
            && batch_info.l1_batch_number == last_committed_l1_batch.next()
        {
            assert!(
                batch_info.base.committed_at.is_some(),
                "Malformed API response: batch is committed, but has no commit timestamp"
            );
            status_changes.commit.push(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.base.commit_tx_hash.unwrap(),
                happened_at: batch_info.base.committed_at.unwrap(),
            });
            tracing::info!("Batch {}: committed", batch_info.l1_batch_number);
            FETCHER_METRICS.l1_batch[&L1BatchStage::Committed]
                .set(batch_info.l1_batch_number.0.into());
            *last_committed_l1_batch += 1;
        }
    }

    fn update_proven_batch(
        status_changes: &mut StatusChanges,
        batch_info: &BlockDetails,
        last_proven_l1_batch: &mut L1BatchNumber,
    ) {
        if batch_info.base.prove_tx_hash.is_some()
            && batch_info.l1_batch_number == last_proven_l1_batch.next()
        {
            assert!(
                batch_info.base.proven_at.is_some(),
                "Malformed API response: batch is proven, but has no prove timestamp"
            );
            status_changes.prove.push(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.base.prove_tx_hash.unwrap(),
                happened_at: batch_info.base.proven_at.unwrap(),
            });
            tracing::info!("Batch {}: proven", batch_info.l1_batch_number);
            FETCHER_METRICS.l1_batch[&L1BatchStage::Proven]
                .set(batch_info.l1_batch_number.0.into());
            *last_proven_l1_batch += 1;
        }
    }

    fn update_executed_batch(
        status_changes: &mut StatusChanges,
        batch_info: &BlockDetails,
        last_executed_l1_batch: &mut L1BatchNumber,
    ) {
        if batch_info.base.execute_tx_hash.is_some()
            && batch_info.l1_batch_number == last_executed_l1_batch.next()
        {
            assert!(
                batch_info.base.executed_at.is_some(),
                "Malformed API response: batch is executed, but has no execute timestamp"
            );
            status_changes.execute.push(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.base.execute_tx_hash.unwrap(),
                happened_at: batch_info.base.executed_at.unwrap(),
            });
            tracing::info!("Batch {}: executed", batch_info.l1_batch_number);
            FETCHER_METRICS.l1_batch[&L1BatchStage::Executed]
                .set(batch_info.l1_batch_number.0.into());
            *last_executed_l1_batch += 1;
        }
    }

    /// Inserts the provided status changes into the database.
    /// This method is not transactional, so it can save only a part of the changes, which is fine:
    /// after the restart the updater will continue from the last saved state.
    ///
    /// The status changes are applied to the database by inserting bogus confirmed transactions (with
    /// some fields missing/substituted) only to satisfy API needs; this component doesn't expect the updated
    /// tables to be ever accessed by the `eth_sender` module.
    async fn apply_status_changes(&mut self, changes: StatusChanges) {
        let total_latency = EN_METRICS.batch_status_updater_loop_iteration.start();
        let mut storage = self.pool.access_storage_tagged("sync_layer").await.unwrap();

        for change in changes.commit.into_iter() {
            tracing::info!(
                "Commit status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
            storage
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::Commit,
                    change.l1_tx_hash,
                    change.happened_at,
                )
                .await
                .unwrap();
            self.last_committed_l1_batch = change.number;
        }
        for change in changes.prove.into_iter() {
            tracing::info!(
                "Prove status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
            storage
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::PublishProofOnchain,
                    change.l1_tx_hash,
                    change.happened_at,
                )
                .await
                .unwrap();
            self.last_proven_l1_batch = change.number;
        }
        for change in changes.execute.into_iter() {
            tracing::info!(
                "Execute status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );

            storage
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::Execute,
                    change.l1_tx_hash,
                    change.happened_at,
                )
                .await
                .unwrap();
            self.last_executed_l1_batch = change.number;
        }

        total_latency.observe();
    }
}
