use std::time::{Duration, Instant};

use tokio::sync::watch::Receiver;

use crate::sync_layer::sync_action::{ActionQueue, SyncAction};
use zksync_dal::MainConnectionPool;
use zksync_types::{L1BatchNumber, MiniblockNumber, H256};
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
        pool: MainConnectionPool,
        main_node_url: &str,
        actions: ActionQueue,
        sync_state: SyncState,
        stop_receiver: Receiver<bool>,
    ) -> Self {
        let mut storage = pool.access_storage_tagged("sync_layer").await.unwrap();
        let last_sealed_l1_batch_header = storage
            .blocks_dal()
            .get_newest_l1_batch_header()
            .await
            .unwrap();
        let last_miniblock_number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .unwrap();

        // It's important to know whether we have opened a new batch already or just sealed the previous one.
        // Depending on it, we must either insert `OpenBatch` item into the queue, or not.
        let was_new_batch_open = storage.blocks_dal().pending_batch_exists().await.unwrap();

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

    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!(
            "Starting the fetcher routine. Initial miniblock: {}, initial l1 batch: {}",
            self.current_miniblock,
            self.current_l1_batch
        );
        // Run the main routine and reconnect upon the network errors.
        loop {
            match self.run_inner().await {
                Ok(()) => {
                    tracing::info!("Stop signal received, exiting the fetcher routine");
                    return Ok(());
                }
                Err(err @ RpcError::Transport(_) | err @ RpcError::RequestTimeout) => {
                    tracing::warn!("Following transport error occurred: {}", err);
                    tracing::info!("Trying again after a delay");
                    tokio::time::sleep(RETRY_DELAY_INTERVAL).await; // TODO (BFT-100): Implement the fibonacci backoff.
                }
                Err(err) => {
                    anyhow::bail!("Unexpected error in the fetcher: {}", err);
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
                tracing::debug!("{log_message}");
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

        // This will be fetched from cache.
        let prev_block = self
            .client
            .sync_l2_block(self.current_miniblock - 1)
            .await?
            .expect("Previous block must exist");

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

            tracing::info!(
                "New batch: {}. Timestamp: {}",
                block.l1_batch_number,
                block.timestamp
            );

            new_actions.push(SyncAction::OpenBatch {
                number: block.l1_batch_number,
                timestamp: block.timestamp,
                l1_gas_price: block.l1_gas_price,
                l2_fair_gas_price: block.l2_fair_gas_price,
                operator_address: block.operator_address,
                protocol_version: block.protocol_version,
                // `block.virtual_blocks` can be `None` only for old VM versions where it's not used, so it's fine to provide any number.
                first_miniblock_info: (block.number, block.virtual_blocks.unwrap_or(0)),
                // Same for `prev_block.hash` as above.
                prev_miniblock_hash: prev_block.hash.unwrap_or_else(H256::zero),
            });
            metrics::gauge!("external_node.fetcher.l1_batch", block.l1_batch_number.0 as f64, "status" => "open");
            self.current_l1_batch += 1;
        } else {
            // New batch implicitly means a new miniblock, so we only need to push the miniblock action
            // if it's not a new batch.
            new_actions.push(SyncAction::Miniblock {
                number: block.number,
                timestamp: block.timestamp,
                // `block.virtual_blocks` can be `None` only for old VM versions where it's not used, so it's fine to provide any number.
                virtual_blocks: block.virtual_blocks.unwrap_or(0),
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
            new_actions.push(SyncAction::SealBatch {
                // `block.virtual_blocks` can be `None` only for old VM versions where it's not used, so it's fine to provide any number.
                virtual_blocks: block.virtual_blocks.unwrap_or(0),
            });
        } else {
            new_actions.push(SyncAction::SealMiniblock);
        }

        tracing::info!(
            "New miniblock: {} / {}",
            block.number,
            self.sync_state.get_main_node_block().max(block.number)
        );
        // Forgetting only the previous one because we still need the current one in cache for the next iteration.
        self.client
            .forget_miniblock(MiniblockNumber(self.current_miniblock.0.saturating_sub(1)));
        self.current_miniblock += 1;
        self.actions.push_actions(new_actions);

        metrics::histogram!(
            "external_node.fetcher.fetch_next_miniblock",
            start.elapsed()
        );
        Ok(true)
    }
}
