use std::time::{Duration, Instant};

use tokio::sync::watch::Receiver;

use crate::sync_layer::sync_action::{ActionQueue, BatchStatusChange, SyncAction};
use zksync_dal::ConnectionPool;
use zksync_types::{explorer_api::BlockDetails, L1BatchNumber, MiniblockNumber};
use zksync_web3_decl::jsonrpsee::core::{Error as RpcError, RpcResult};

use super::{cached_main_node_client::CachedMainNodeClient, SyncState};

const DELAY_INTERVAL: Duration = Duration::from_millis(500);
const RETRY_DELAY_INTERVAL: Duration = Duration::from_secs(5);

/// Structure responsible for fetching batches and miniblock data from the main node.
#[derive(Debug)]
pub struct MainNodeFetcher {
    client: CachedMainNodeClient,
    current_l1_batch: L1BatchNumber,
    current_miniblock: MiniblockNumber,

    last_executed_l1_batch: L1BatchNumber,
    last_proven_l1_batch: L1BatchNumber,
    last_committed_l1_batch: L1BatchNumber,

    actions: ActionQueue,
    sync_state: SyncState,
    stop_receiver: Receiver<bool>,
}

impl MainNodeFetcher {
    pub fn new(
        pool: ConnectionPool,
        main_node_url: &str,
        actions: ActionQueue,
        sync_state: SyncState,
        stop_receiver: Receiver<bool>,
    ) -> Self {
        let mut storage = pool.access_storage_blocking();
        let last_sealed_block_header = storage.blocks_dal().get_newest_block_header();
        let last_miniblock_number = storage.blocks_dal().get_sealed_miniblock_number();

        // It's important to know whether we have opened a new batch already or just sealed the previous one.
        // Depending on it, we must either insert `OpenBatch` item into the queue, or not.
        let was_new_batch_open = storage.blocks_dal().pending_batch_exists();

        // Miniblocks are always fully processed.
        let current_miniblock = last_miniblock_number + 1;
        // Decide whether the next batch should be explicitly opened or not.
        let current_l1_batch = if was_new_batch_open {
            // No `OpenBatch` action needed.
            last_sealed_block_header.number + 1
        } else {
            // We need to open the next batch.
            last_sealed_block_header.number
        };

        let last_executed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_block_executed_on_eth()
            .unwrap_or_default();
        let last_proven_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_block_proven_on_eth()
            .unwrap_or_default();
        let last_committed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_block_committed_on_eth()
            .unwrap_or_default();

        let client = CachedMainNodeClient::build_client(main_node_url);

        Self {
            client,
            current_l1_batch,
            current_miniblock,

            last_executed_l1_batch,
            last_proven_l1_batch,
            last_committed_l1_batch,

            actions,
            sync_state,
            stop_receiver,
        }
    }

    pub async fn run(mut self) {
        vlog::info!(
            "Starting the fetcher routine. Initial miniblock: {}, initial l1 batch: {}",
            self.current_miniblock,
            self.current_l1_batch
        );
        // Run the main routine and reconnect upon the network errors.
        loop {
            match self.run_inner().await {
                Ok(()) => {
                    vlog::info!("Stop signal received, exiting the fetcher routine");
                    return;
                }
                Err(err @ RpcError::Transport(_) | err @ RpcError::RequestTimeout) => {
                    vlog::warn!("Following transport error occurred: {}", err);
                    vlog::info!("Trying again after a delay");
                    tokio::time::sleep(RETRY_DELAY_INTERVAL).await;
                }
                Err(err) => {
                    panic!("Unexpected error in the fetcher: {}", err);
                }
            }
        }
    }

    fn check_if_cancelled(&self) -> bool {
        *self.stop_receiver.borrow()
    }

    async fn run_inner(&mut self) -> RpcResult<()> {
        loop {
            if self.check_if_cancelled() {
                return Ok(());
            }

            let mut progressed = false;

            let last_main_node_block =
                MiniblockNumber(self.client.get_block_number().await?.as_u32());
            self.sync_state.set_main_node_block(last_main_node_block);

            self.client
                .populate_miniblocks_cache(self.current_miniblock, last_main_node_block)
                .await;
            if self.actions.has_action_capacity() {
                progressed |= self.fetch_next_miniblock().await?;
            }
            if self.actions.has_status_change_capacity() {
                progressed |= self.update_batch_statuses().await?;
            }

            if !progressed {
                // We didn't fetch any updated on this iteration, so to prevent a busy loop we wait a bit.
                vlog::debug!("No updates to discover, waiting");
                std::thread::sleep(DELAY_INTERVAL);
            }
        }
    }

    /// Tries to fetch the next miniblock and insert it to the sync queue.
    /// Returns `true` if a miniblock was processed and `false` otherwise.
    async fn fetch_next_miniblock(&mut self) -> RpcResult<bool> {
        let start = Instant::now();

        let request_start = Instant::now();
        let Some(miniblock_header) = self
                .client
                .get_block_details(self.current_miniblock)
                .await?
            else {
                return Ok(false);
            };
        metrics::histogram!(
            "external_node.fetcher.requests",
            request_start.elapsed(),
            "stage" => "get_block_details",
            "actor" => "miniblock_fetcher"
        );

        let mut new_actions = Vec::new();
        if miniblock_header.l1_batch_number != self.current_l1_batch {
            assert_eq!(
                miniblock_header.l1_batch_number,
                self.current_l1_batch.next(),
                "Unexpected batch number in the next received miniblock"
            );

            vlog::info!(
                "New batch: {}. Timestamp: {}",
                miniblock_header.l1_batch_number,
                miniblock_header.timestamp
            );

            new_actions.push(SyncAction::OpenBatch {
                number: miniblock_header.l1_batch_number,
                timestamp: miniblock_header.timestamp,
                l1_gas_price: miniblock_header.l1_gas_price,
                l2_fair_gas_price: miniblock_header.l2_fair_gas_price,
                base_system_contracts_hashes: miniblock_header.base_system_contracts_hashes,
                operator_address: miniblock_header.operator_address,
            });
            metrics::gauge!("external_node.fetcher.l1_batch", miniblock_header.l1_batch_number.0 as f64, "status" => "open");

            self.client.forget_l1_batch(self.current_l1_batch);
            self.current_l1_batch += 1;
        } else {
            // New batch implicitly means a new miniblock, so we only need to push the miniblock action
            // if it's not a new batch.
            new_actions.push(SyncAction::Miniblock {
                number: miniblock_header.number,
                timestamp: miniblock_header.timestamp,
            });
            metrics::gauge!(
                "external_node.fetcher.miniblock",
                miniblock_header.number.0 as f64
            );
        }

        let request_start = Instant::now();
        let miniblock_txs = self
            .client
            .get_raw_block_transactions(self.current_miniblock)
            .await?
            .into_iter()
            .map(|tx| SyncAction::Tx(Box::new(tx)));
        metrics::histogram!(
            "external_node.fetcher.requests",
            request_start.elapsed(),
            "stage" => "get_raw_block_transactions",
            "actor" => "miniblock_fetcher"
        );

        metrics::counter!(
            "server.processed_txs",
            miniblock_txs.len() as u64,
            "stage" => "mempool_added"
        );
        new_actions.extend(miniblock_txs);

        // Check if this was the last miniblock in the batch.
        // If we will receive `None` here, it would mean that it's the currently open batch and it was not sealed
        // after the current miniblock.
        let request_start = Instant::now();
        let is_last_miniblock_of_batch = self
            .client
            .get_miniblock_range(self.current_l1_batch)
            .await?
            .map(|(_, last)| last.as_u32() == miniblock_header.number.0)
            .unwrap_or(false);
        metrics::histogram!(
            "external_node.fetcher.requests",
            request_start.elapsed(),
            "stage" => "get_miniblock_range",
            "actor" => "miniblock_fetcher"
        );

        // Last miniblock of the batch is a "fictive" miniblock and would be replicated locally.
        // We don't need to seal it explicitly, so we only put the seal miniblock command if it's not the last miniblock.
        if is_last_miniblock_of_batch {
            new_actions.push(SyncAction::SealBatch);
        } else {
            new_actions.push(SyncAction::SealMiniblock);
        }

        vlog::info!("New miniblock: {}", miniblock_header.number);
        self.client.forget_miniblock(self.current_miniblock);
        self.current_miniblock += 1;
        self.actions.push_actions(new_actions);

        metrics::histogram!(
            "external_node.fetcher.fetch_next_miniblock",
            start.elapsed()
        );
        Ok(true)
    }

    /// Goes through the already fetched batches trying to update their statuses.
    /// Returns `true` if at least one batch was updated, and `false` otherwise.
    async fn update_batch_statuses(&mut self) -> RpcResult<bool> {
        let start = Instant::now();
        assert!(
            self.last_executed_l1_batch <= self.last_proven_l1_batch,
            "Incorrect local state: executed batch must be proven"
        );
        assert!(
            self.last_proven_l1_batch <= self.last_committed_l1_batch,
            "Incorrect local state: proven batch must be committed"
        );
        assert!(
            self.last_committed_l1_batch <= self.current_l1_batch,
            "Incorrect local state: unkonwn batch marked as committed"
        );

        let mut applied_updates = false;
        let mut batch = self.last_executed_l1_batch.next();
        // In this loop we try to progress on the batch statuses, utilizing the same request to the node to potentially
        // update all three statuses (e.g. if the node is still syncing), but also skipping the gaps in the statuses
        // (e.g. if the last executed batch is 10, but the last proven is 20, we don't need to check the batches 11-19).
        while batch <= self.current_l1_batch {
            // While we may receive `None` for the `self.current_l1_batch`, it's OK: open batch is guaranteed to not
            // be sent to L1.
            let request_start = Instant::now();
            let Some((start_miniblock, _)) = self.client.get_miniblock_range(batch).await? else {
                return Ok(applied_updates);
            };
            metrics::histogram!(
                "external_node.fetcher.requests",
                request_start.elapsed(),
                "stage" => "get_miniblock_range",
                "actor" => "batch_status_fetcher"
            );

            // We could've used any miniblock from the range, all of them share the same info.
            let request_start = Instant::now();
            let Some(batch_info) = self
                .client
                .get_block_details(MiniblockNumber(start_miniblock.as_u32()))
                .await?
            else {
                // We cannot recover from an external API inconsistency.
                panic!(
                    "Node API is inconsistent: miniblock {} was reported to be a part of {} L1batch, \
                    but API has no information about this miniblock", start_miniblock, batch
                );
            };
            metrics::histogram!(
                "external_node.fetcher.requests",
                request_start.elapsed(),
                "stage" => "get_block_details",
                "actor" => "batch_status_fetcher"
            );

            applied_updates |= self.update_committed_batch(&batch_info);
            applied_updates |= self.update_proven_batch(&batch_info);
            applied_updates |= self.update_executed_batch(&batch_info);

            // Check whether we can skip a part of the range.
            if batch_info.commit_tx_hash.is_none() {
                // No committed batches after this one.
                break;
            } else if batch_info.prove_tx_hash.is_none() && batch < self.last_committed_l1_batch {
                // The interval between this batch and the last committed one is not proven.
                batch = self.last_committed_l1_batch.next();
            } else if batch_info.executed_at.is_none() && batch < self.last_proven_l1_batch {
                // The interval between this batch and the last proven one is not executed.
                batch = self.last_proven_l1_batch.next();
            } else {
                batch += 1;
            }
        }

        metrics::histogram!("external_node.update_batch_statuses", start.elapsed());
        Ok(applied_updates)
    }

    /// Returns `true` if batch info was updated.
    fn update_committed_batch(&mut self, batch_info: &BlockDetails) -> bool {
        if batch_info.commit_tx_hash.is_some()
            && batch_info.l1_batch_number == self.last_committed_l1_batch.next()
        {
            assert!(
                batch_info.committed_at.is_some(),
                "Malformed API response: batch is committed, but has no commit timestamp"
            );
            self.actions.push_commit_status_change(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.commit_tx_hash.unwrap(),
                happened_at: batch_info.committed_at.unwrap(),
            });
            vlog::info!("Batch {}: committed", batch_info.l1_batch_number);
            metrics::gauge!("external_node.fetcher.l1_batch", batch_info.l1_batch_number.0 as f64, "status" => "committed");
            self.last_committed_l1_batch += 1;
            true
        } else {
            false
        }
    }

    /// Returns `true` if batch info was updated.
    fn update_proven_batch(&mut self, batch_info: &BlockDetails) -> bool {
        if batch_info.prove_tx_hash.is_some()
            && batch_info.l1_batch_number == self.last_proven_l1_batch.next()
        {
            assert!(
                batch_info.proven_at.is_some(),
                "Malformed API response: batch is proven, but has no prove timestamp"
            );
            self.actions.push_prove_status_change(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.prove_tx_hash.unwrap(),
                happened_at: batch_info.proven_at.unwrap(),
            });
            vlog::info!("Batch {}: proven", batch_info.l1_batch_number);
            metrics::gauge!("external_node.fetcher.l1_batch", batch_info.l1_batch_number.0 as f64, "status" => "proven");
            self.last_proven_l1_batch += 1;
            true
        } else {
            false
        }
    }

    /// Returns `true` if batch info was updated.
    fn update_executed_batch(&mut self, batch_info: &BlockDetails) -> bool {
        if batch_info.execute_tx_hash.is_some()
            && batch_info.l1_batch_number == self.last_executed_l1_batch.next()
        {
            assert!(
                batch_info.executed_at.is_some(),
                "Malformed API response: batch is executed, but has no execute timestamp"
            );
            self.actions.push_execute_status_change(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.execute_tx_hash.unwrap(),
                happened_at: batch_info.executed_at.unwrap(),
            });
            vlog::info!("Batch {}: executed", batch_info.l1_batch_number);
            metrics::gauge!("external_node.fetcher.l1_batch", batch_info.l1_batch_number.0 as f64, "status" => "executed");
            self.last_executed_l1_batch += 1;
            true
        } else {
            false
        }
    }
}
