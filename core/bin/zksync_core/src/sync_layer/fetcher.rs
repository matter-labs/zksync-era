use std::time::{Duration, Instant};

use tokio::sync::watch::Receiver;

use crate::sync_layer::sync_action::{ActionQueue, SyncAction};
use zksync_dal::ConnectionPool;
use zksync_types::{L1BatchNumber, MiniblockNumber};
use zksync_web3_decl::jsonrpsee::core::Error as RpcError;
use zksync_web3_decl::RpcResult;

use super::{cached_main_node_client::CachedMainNodeClient, SyncState};

const DELAY_INTERVAL: Duration = Duration::from_millis(500);
const RETRY_DELAY_INTERVAL: Duration = Duration::from_secs(5);

/// Structure responsible for fetching batches and miniblock data from the main node.
#[derive(Debug)]
pub struct MainNodeFetcher {
    client: CachedMainNodeClient,
    current_l1_batch: L1BatchNumber,
    current_miniblock: MiniblockNumber,

    actions: ActionQueue,
    sync_state: SyncState,
    stop_receiver: Receiver<bool>,
}

impl MainNodeFetcher {
    pub async fn new(
        pool: ConnectionPool,
        main_node_url: &str,
        actions: ActionQueue,
        sync_state: SyncState,
        stop_receiver: Receiver<bool>,
    ) -> Self {
        let mut storage = pool.access_storage_tagged("sync_layer").await;
        let last_sealed_l1_batch_header = storage.blocks_dal().get_newest_l1_batch_header().await;
        let last_miniblock_number = storage.blocks_dal().get_sealed_miniblock_number().await;

        // It's important to know whether we have opened a new batch already or just sealed the previous one.
        // Depending on it, we must either insert `OpenBatch` item into the queue, or not.
        let was_new_batch_open = storage.blocks_dal().pending_batch_exists().await;

        // Miniblocks are always fully processed.
        let current_miniblock = last_miniblock_number + 1;
        // Decide whether the next batch should be explicitly opened or not.
        let current_l1_batch = if was_new_batch_open {
            // No `OpenBatch` action needed.
            last_sealed_l1_batch_header.number + 1
        } else {
            // We need to open the next batch.
            last_sealed_l1_batch_header.number
        };

        let client = CachedMainNodeClient::build_client(main_node_url);

        Self {
            client,
            current_l1_batch,
            current_miniblock,

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
            let has_action_capacity = self.actions.has_action_capacity();
            if has_action_capacity {
                progressed |= self.fetch_next_miniblock().await?;
            }

            if !progressed {
                // We didn't fetch any updated on this iteration, so to prevent a busy loop we wait a bit.
                let log_message = if has_action_capacity {
                    "No updates to discover, waiting for new blocks on the main node"
                } else {
                    "Local action queue is full, waiting for state keeper to process the queue"
                };
                vlog::debug!("{log_message}");
                tokio::time::sleep(DELAY_INTERVAL).await;
            }
        }
    }

    /// Tries to fetch the next miniblock and insert it to the sync queue.
    /// Returns `true` if a miniblock was processed and `false` otherwise.
    async fn fetch_next_miniblock(&mut self) -> RpcResult<bool> {
        let start = Instant::now();

        let request_start = Instant::now();
        let Some(block) = self.client.sync_l2_block(self.current_miniblock).await? else {
            return Ok(false);
        };
        metrics::histogram!(
            "external_node.fetcher.requests",
            request_start.elapsed(),
            "stage" => "sync_l2_block",
            "actor" => "miniblock_fetcher"
        );

        let mut new_actions = Vec::new();
        if block.l1_batch_number != self.current_l1_batch {
            assert_eq!(
                block.l1_batch_number,
                self.current_l1_batch.next(),
                "Unexpected batch number in the next received miniblock"
            );

            vlog::info!(
                "New batch: {}. Timestamp: {}",
                block.l1_batch_number,
                block.timestamp
            );

            new_actions.push(SyncAction::OpenBatch {
                number: block.l1_batch_number,
                timestamp: block.timestamp,
                l1_gas_price: block.l1_gas_price,
                l2_fair_gas_price: block.l2_fair_gas_price,
                base_system_contracts_hashes: block.base_system_contracts_hashes,
                operator_address: block.operator_address,
                protocol_version: None,
            });
            metrics::gauge!("external_node.fetcher.l1_batch", block.l1_batch_number.0 as f64, "status" => "open");
            self.current_l1_batch += 1;
        } else {
            // New batch implicitly means a new miniblock, so we only need to push the miniblock action
            // if it's not a new batch.
            new_actions.push(SyncAction::Miniblock {
                number: block.number,
                timestamp: block.timestamp,
            });
            metrics::gauge!("external_node.fetcher.miniblock", block.number.0 as f64);
        }

        let txs: Vec<zksync_types::Transaction> = block
            .transactions
            .expect("Transactions are always requested");
        metrics::counter!(
            "server.processed_txs",
            txs.len() as u64,
            "stage" => "mempool_added"
        );
        new_actions.extend(txs.into_iter().map(SyncAction::from));

        // Last miniblock of the batch is a "fictive" miniblock and would be replicated locally.
        // We don't need to seal it explicitly, so we only put the seal miniblock command if it's not the last miniblock.
        if block.last_in_batch {
            new_actions.push(SyncAction::SealBatch);
        } else {
            new_actions.push(SyncAction::SealMiniblock);
        }

        vlog::info!(
            "New miniblock: {} / {}",
            block.number,
            self.sync_state.get_main_node_block().max(block.number)
        );
        self.client.forget_miniblock(self.current_miniblock);
        self.current_miniblock += 1;
        self.actions.push_actions(new_actions);

        metrics::histogram!(
            "external_node.fetcher.fetch_next_miniblock",
            start.elapsed()
        );
        Ok(true)
    }
}
