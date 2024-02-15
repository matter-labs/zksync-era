use std::time::Duration;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::StorageProcessor;
use zksync_types::{
    api::en::SyncBlock, block::MiniblockHasher, Address, L1BatchNumber, MiniblockNumber,
    ProtocolVersionId, H256,
};
use zksync_web3_decl::{error::EnrichedClientError, jsonrpsee::core::ClientError as RpcError};

use super::{
    client::{CachingMainNodeClient, MainNodeClient},
    metrics::{FetchStage, L1BatchStage, FETCHER_METRICS},
    sync_action::{ActionQueueSender, SyncAction},
    SyncState,
};
use crate::{
    metrics::{TxStage, APP_METRICS},
    state_keeper::io::common::IoCursor,
};

const DELAY_INTERVAL: Duration = Duration::from_millis(500);
const RETRY_DELAY_INTERVAL: Duration = Duration::from_secs(5);

/// Common denominator for blocks fetched by an external node.
#[derive(Debug)]
pub(crate) struct FetchedBlock {
    pub number: MiniblockNumber,
    pub l1_batch_number: L1BatchNumber,
    pub last_in_batch: bool,
    pub protocol_version: ProtocolVersionId,
    pub timestamp: u64,
    pub reference_hash: Option<H256>,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub fair_pubdata_price: Option<u64>,
    pub virtual_blocks: u32,
    pub operator_address: Address,
    pub transactions: Vec<zksync_types::Transaction>,
}

impl FetchedBlock {
    fn compute_hash(&self, prev_miniblock_hash: H256) -> H256 {
        let mut hasher = MiniblockHasher::new(self.number, self.timestamp, prev_miniblock_hash);
        for tx in &self.transactions {
            hasher.push_tx_hash(tx.hash());
        }
        hasher.finalize(self.protocol_version)
    }
}

impl TryFrom<SyncBlock> for FetchedBlock {
    type Error = anyhow::Error;

    fn try_from(block: SyncBlock) -> anyhow::Result<Self> {
        Ok(Self {
            number: block.number,
            l1_batch_number: block.l1_batch_number,
            last_in_batch: block.last_in_batch,
            protocol_version: block.protocol_version,
            timestamp: block.timestamp,
            reference_hash: block.hash,
            l1_gas_price: block.l1_gas_price,
            l2_fair_gas_price: block.l2_fair_gas_price,
            fair_pubdata_price: block.fair_pubdata_price,
            virtual_blocks: block.virtual_blocks.unwrap_or(0),
            operator_address: block.operator_address,
            transactions: block
                .transactions
                .context("Transactions are always requested")?,
        })
    }
}

impl IoCursor {
    /// Loads this cursor from storage and modifies it to account for the pending L1 batch if necessary.
    pub(crate) async fn for_fetcher(storage: &mut StorageProcessor<'_>) -> anyhow::Result<Self> {
        let mut this = Self::new(storage).await?;
        // It's important to know whether we have opened a new batch already or just sealed the previous one.
        // Depending on it, we must either insert `OpenBatch` item into the queue, or not.
        let was_new_batch_open = storage
            .blocks_dal()
            .pending_batch_exists()
            .await
            .context("Failed checking whether pending L1 batch exists")?;
        if !was_new_batch_open {
            this.l1_batch -= 1; // Should continue from the last L1 batch present in the storage
        }
        Ok(this)
    }

    pub(crate) fn advance(&mut self, block: FetchedBlock) -> Vec<SyncAction> {
        assert_eq!(block.number, self.next_miniblock);
        let local_block_hash = block.compute_hash(self.prev_miniblock_hash);
        if let Some(reference_hash) = block.reference_hash {
            if local_block_hash != reference_hash {
                // This is a warning, not an assertion because hash mismatch may occur after a reorg.
                // Indeed, `self.prev_miniblock_hash` may differ from the hash of the updated previous miniblock.
                tracing::warn!(
                    "Mismatch between the locally computed and received miniblock hash for {block:?}; \
                     local_block_hash = {local_block_hash:?}, prev_miniblock_hash = {:?}",
                    self.prev_miniblock_hash
                );
            }
        }

        let mut new_actions = Vec::new();
        if block.l1_batch_number != self.l1_batch {
            assert_eq!(
                block.l1_batch_number,
                self.l1_batch.next(),
                "Unexpected batch number in the next received miniblock"
            );

            tracing::info!(
                "New L1 batch: {}. Timestamp: {}",
                block.l1_batch_number,
                block.timestamp
            );

            new_actions.push(SyncAction::OpenBatch {
                number: block.l1_batch_number,
                timestamp: block.timestamp,
                l1_gas_price: block.l1_gas_price,
                l2_fair_gas_price: block.l2_fair_gas_price,
                fair_pubdata_price: block.fair_pubdata_price,
                operator_address: block.operator_address,
                protocol_version: block.protocol_version,
                // `block.virtual_blocks` can be `None` only for old VM versions where it's not used, so it's fine to provide any number.
                first_miniblock_info: (block.number, block.virtual_blocks),
            });
            FETCHER_METRICS.l1_batch[&L1BatchStage::Open].set(block.l1_batch_number.0.into());
            self.l1_batch += 1;
        } else {
            // New batch implicitly means a new miniblock, so we only need to push the miniblock action
            // if it's not a new batch.
            new_actions.push(SyncAction::Miniblock {
                number: block.number,
                timestamp: block.timestamp,
                // `block.virtual_blocks` can be `None` only for old VM versions where it's not used, so it's fine to provide any number.
                virtual_blocks: block.virtual_blocks,
            });
            FETCHER_METRICS.miniblock.set(block.number.0.into());
        }

        APP_METRICS.processed_txs[&TxStage::added_to_mempool()]
            .inc_by(block.transactions.len() as u64);
        new_actions.extend(block.transactions.into_iter().map(SyncAction::from));

        // Last miniblock of the batch is a "fictive" miniblock and would be replicated locally.
        // We don't need to seal it explicitly, so we only put the seal miniblock command if it's not the last miniblock.
        if block.last_in_batch {
            new_actions.push(SyncAction::SealBatch {
                // `block.virtual_blocks` can be `None` only for old VM versions where it's not used, so it's fine to provide any number.
                virtual_blocks: block.virtual_blocks,
            });
        } else {
            new_actions.push(SyncAction::SealMiniblock);
        }
        self.next_miniblock += 1;
        self.prev_miniblock_hash = local_block_hash;

        new_actions
    }
}

/// Errors that can be emitted by [`MainNodeFetcher`].
#[derive(Debug, thiserror::Error)]
enum FetcherError {
    #[error("JSON-RPC error communicating with main node")]
    Web3(#[from] EnrichedClientError),
    #[error("internal fetcher error")]
    Internal(#[from] anyhow::Error),
}

impl FetcherError {
    fn is_transient(&self) -> bool {
        match self {
            Self::Web3(err) => {
                matches!(
                    err.as_ref(),
                    RpcError::Transport(_) | RpcError::RequestTimeout
                )
            }
            Self::Internal(_) => false,
        }
    }
}

/// Structure responsible for fetching batches and miniblock data from the main node.
#[derive(Debug)]
pub struct MainNodeFetcher {
    // Fields are public for testing purposes.
    pub(super) client: CachingMainNodeClient,
    pub(super) cursor: IoCursor,
    pub(super) actions: ActionQueueSender,
    pub(super) sync_state: SyncState,
    pub(super) stop_receiver: watch::Receiver<bool>,
}

impl MainNodeFetcher {
    pub async fn new(
        storage: &mut StorageProcessor<'_>,
        client: Box<dyn MainNodeClient>,
        actions: ActionQueueSender,
        sync_state: SyncState,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<Self> {
        let cursor = IoCursor::for_fetcher(storage)
            .await
            .context("failed getting I/O cursor from Postgres")?;
        Ok(Self {
            client: CachingMainNodeClient::new(client),
            cursor,
            actions,
            sync_state,
            stop_receiver,
        })
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!(
            "Starting the fetcher routine. Initial miniblock: {}, initial l1 batch: {}",
            self.cursor.next_miniblock,
            self.cursor.l1_batch
        );
        // Run the main routine and reconnect upon the network errors.
        loop {
            match self.run_inner().await {
                Ok(()) => {
                    tracing::info!("Stop signal received, exiting the fetcher routine");
                    return Ok(());
                }
                Err(err) if err.is_transient() => {
                    tracing::warn!("Following transport error occurred: {err}");
                    tracing::info!("Trying again after a delay: {RETRY_DELAY_INTERVAL:?}");
                    tokio::time::sleep(RETRY_DELAY_INTERVAL).await; // TODO (BFT-100): Implement the Fibonacci back-off.
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    fn check_if_cancelled(&self) -> bool {
        *self.stop_receiver.borrow()
    }

    async fn run_inner(&mut self) -> Result<(), FetcherError> {
        loop {
            if self.check_if_cancelled() {
                return Ok(());
            }

            let mut progressed = false;
            let last_main_node_block = self.client.fetch_l2_block_number().await?;
            self.sync_state.set_main_node_block(last_main_node_block);

            self.client
                .populate_miniblocks_cache(self.cursor.next_miniblock, last_main_node_block)
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
    async fn fetch_next_miniblock(&mut self) -> Result<bool, FetcherError> {
        let total_latency = FETCHER_METRICS.fetch_next_miniblock.start();
        let request_latency = FETCHER_METRICS.requests[&FetchStage::SyncL2Block].start();
        let Some(block) = self
            .client
            .fetch_l2_block(self.cursor.next_miniblock)
            .await?
        else {
            return Ok(false);
        };
        request_latency.observe();

        let block_number = block.number;
        let new_actions = self.cursor.advance(block.try_into()?);

        tracing::info!(
            "New miniblock: {block_number} / {}",
            self.sync_state.get_main_node_block().max(block_number)
        );
        // Forgetting only the previous one because we still need the current one in cache for the next iteration.
        let prev_miniblock_number = MiniblockNumber(block_number.0.saturating_sub(1));
        self.client.forget_miniblock(prev_miniblock_number);
        self.actions.push_actions(new_actions).await;

        total_latency.observe();
        Ok(true)
    }
}
