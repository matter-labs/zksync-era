use anyhow::Context as _;
use tokio::sync::watch;

use std::time::Duration;

use zksync_dal::StorageProcessor;
use zksync_types::{L1BatchNumber, MiniblockNumber, H256};
use zksync_web3_decl::jsonrpsee::core::Error as RpcError;

use super::{
    client::{CachingMainNodeClient, MainNodeClient},
    metrics::{FetchStage, L1BatchStage, FETCHER_METRICS},
    sync_action::{ActionQueueSender, SyncAction},
    SyncState,
};
use crate::metrics::{TxStage, APP_METRICS};

const DELAY_INTERVAL: Duration = Duration::from_millis(500);
const RETRY_DELAY_INTERVAL: Duration = Duration::from_secs(5);

/// Cursor of [`MainNodeFetcher`].
#[derive(Debug)]
pub struct MainNodeFetcherCursor {
    // Fields are public for testing purposes.
    pub(super) miniblock: MiniblockNumber,
    pub(super) l1_batch: L1BatchNumber,
}

impl MainNodeFetcherCursor {
    /// Loads the cursor
    pub async fn new(storage: &mut StorageProcessor<'_>) -> anyhow::Result<Self> {
        let last_sealed_l1_batch_header = storage
            .blocks_dal()
            .get_newest_l1_batch_header()
            .await
            .context("Failed getting newest L1 batch header")?;
        let last_miniblock_number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .context("Failed getting sealed miniblock number")?;

        // It's important to know whether we have opened a new batch already or just sealed the previous one.
        // Depending on it, we must either insert `OpenBatch` item into the queue, or not.
        let was_new_batch_open = storage
            .blocks_dal()
            .pending_batch_exists()
            .await
            .context("Failed checking whether pending L1 batch exists")?;

        // Miniblocks are always fully processed.
        let miniblock = last_miniblock_number + 1;
        // Decide whether the next batch should be explicitly opened or not.
        let l1_batch = if was_new_batch_open {
            // No `OpenBatch` action needed.
            last_sealed_l1_batch_header.number + 1
        } else {
            // We need to open the next batch.
            last_sealed_l1_batch_header.number
        };

        Ok(Self {
            miniblock,
            l1_batch,
        })
    }

    /// Builds a fetcher from this cursor.
    pub fn into_fetcher(
        self,
        client: Box<dyn MainNodeClient>,
        actions: ActionQueueSender,
        sync_state: SyncState,
        stop_receiver: watch::Receiver<bool>,
    ) -> MainNodeFetcher {
        MainNodeFetcher {
            client: CachingMainNodeClient::new(client),
            cursor: self,
            actions,
            sync_state,
            stop_receiver,
        }
    }
}

/// Structure responsible for fetching batches and miniblock data from the main node.
#[derive(Debug)]
pub struct MainNodeFetcher {
    client: CachingMainNodeClient,
    cursor: MainNodeFetcherCursor,
    actions: ActionQueueSender,
    sync_state: SyncState,
    stop_receiver: watch::Receiver<bool>,
}

impl MainNodeFetcher {
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!(
            "Starting the fetcher routine. Initial miniblock: {}, initial l1 batch: {}",
            self.cursor.miniblock,
            self.cursor.l1_batch
        );
        // Run the main routine and reconnect upon the network errors.
        loop {
            match self.run_inner().await {
                Ok(()) => {
                    tracing::info!("Stop signal received, exiting the fetcher routine");
                    return Ok(());
                }
                Err(err) => {
                    if let Some(err @ RpcError::Transport(_) | err @ RpcError::RequestTimeout) =
                        err.downcast_ref::<RpcError>()
                    {
                        tracing::warn!("Following transport error occurred: {err}");
                        tracing::info!("Trying again after a delay");
                        tokio::time::sleep(RETRY_DELAY_INTERVAL).await; // TODO (BFT-100): Implement the fibonacci backoff.
                    } else {
                        return Err(err.context("Unexpected error in the fetcher"));
                    }
                }
            }
        }
    }

    fn check_if_cancelled(&self) -> bool {
        *self.stop_receiver.borrow()
    }

    async fn run_inner(&mut self) -> anyhow::Result<()> {
        loop {
            if self.check_if_cancelled() {
                return Ok(());
            }

            let mut progressed = false;
            let last_main_node_block = self.client.fetch_l2_block_number().await?;
            self.sync_state.set_main_node_block(last_main_node_block);

            self.client
                .populate_miniblocks_cache(self.cursor.miniblock, last_main_node_block)
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
    async fn fetch_next_miniblock(&mut self) -> anyhow::Result<bool> {
        let total_latency = FETCHER_METRICS.fetch_next_miniblock.start();
        let request_latency = FETCHER_METRICS.requests[&FetchStage::SyncL2Block].start();
        let Some(block) = self.client.fetch_l2_block(self.cursor.miniblock).await? else {
            return Ok(false);
        };

        // This will be fetched from cache.
        let prev_block = self
            .client
            .fetch_l2_block(self.cursor.miniblock - 1)
            .await?
            .expect("Previous block must exist");
        request_latency.observe();

        let mut new_actions = Vec::new();
        if block.l1_batch_number != self.cursor.l1_batch {
            assert_eq!(
                block.l1_batch_number,
                self.cursor.l1_batch.next(),
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
            FETCHER_METRICS.l1_batch[&L1BatchStage::Open].set(block.l1_batch_number.0.into());
            self.cursor.l1_batch += 1;
        } else {
            // New batch implicitly means a new miniblock, so we only need to push the miniblock action
            // if it's not a new batch.
            new_actions.push(SyncAction::Miniblock {
                number: block.number,
                timestamp: block.timestamp,
                // `block.virtual_blocks` can be `None` only for old VM versions where it's not used, so it's fine to provide any number.
                virtual_blocks: block.virtual_blocks.unwrap_or(0),
            });
            FETCHER_METRICS.miniblock.set(block.number.0.into());
        }

        let txs: Vec<zksync_types::Transaction> = block
            .transactions
            .expect("Transactions are always requested");
        APP_METRICS.processed_txs[&TxStage::added_to_mempool()].inc_by(txs.len() as u64);
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
        let prev_miniblock_number = MiniblockNumber(self.cursor.miniblock.0.saturating_sub(1));
        self.client.forget_miniblock(prev_miniblock_number);
        self.cursor.miniblock += 1;
        self.actions.push_actions(new_actions).await;

        total_latency.observe();
        Ok(true)
    }
}
