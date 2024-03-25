use anyhow::Context as _;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{
    api::en::SyncBlock, block::MiniblockHasher, fee_model::BatchFeeInput,
    helpers::unix_timestamp_ms, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, H256,
};

use super::{
    metrics::{L1BatchStage, FETCHER_METRICS},
    sync_action::SyncAction,
};
use crate::{
    metrics::{TxStage, APP_METRICS},
    state_keeper::io::{common::IoCursor, L1BatchParams, MiniblockParams},
};

/// Same as [`zksync_types::Transaction`], just with additional guarantees that the "received at" timestamp was set locally.
/// We cannot transfer `Transaction`s without these timestamps, because this would break backward compatibility.
#[derive(Debug, Clone)]
pub(crate) struct FetchedTransaction(zksync_types::Transaction);

impl FetchedTransaction {
    pub fn new(mut tx: zksync_types::Transaction) -> Self {
        // Override the "received at" timestamp for the transaction so that they are causally ordered (i.e., transactions
        // with an earlier timestamp are persisted earlier). Without this property, code relying on causal ordering may work incorrectly;
        // e.g., `pendingTransactions` subscriptions notifier can skip transactions.
        tx.received_timestamp_ms = unix_timestamp_ms();
        Self(tx)
    }

    pub fn hash(&self) -> H256 {
        self.0.hash()
    }
}

impl From<FetchedTransaction> for zksync_types::Transaction {
    fn from(tx: FetchedTransaction) -> Self {
        tx.0
    }
}

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
    pub transactions: Vec<FetchedTransaction>,
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
        let Some(transactions) = block.transactions else {
            return Err(anyhow::anyhow!("Transactions are always requested"));
        };

        if transactions.is_empty() && !block.last_in_batch {
            return Err(anyhow::anyhow!(
                "Only last miniblock of the batch can be empty"
            ));
        }

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
            transactions: transactions
                .into_iter()
                .map(FetchedTransaction::new)
                .collect(),
        })
    }
}

impl IoCursor {
    /// Loads this cursor from storage and modifies it to account for the pending L1 batch if necessary.
    pub(crate) async fn for_fetcher(storage: &mut Connection<'_, Core>) -> anyhow::Result<Self> {
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
                params: L1BatchParams {
                    protocol_version: block.protocol_version,
                    validation_computational_gas_limit: super::VALIDATION_COMPUTATIONAL_GAS_LIMIT,
                    operator_address: block.operator_address,
                    fee_input: BatchFeeInput::for_protocol_version(
                        block.protocol_version,
                        block.l2_fair_gas_price,
                        block.fair_pubdata_price,
                        block.l1_gas_price,
                    ),
                    first_miniblock: MiniblockParams {
                        timestamp: block.timestamp,
                        virtual_blocks: block.virtual_blocks,
                    },
                },
                number: block.l1_batch_number,
                first_miniblock_number: block.number,
            });
            FETCHER_METRICS.l1_batch[&L1BatchStage::Open].set(block.l1_batch_number.0.into());
            self.l1_batch += 1;
        } else {
            // New batch implicitly means a new miniblock, so we only need to push the miniblock action
            // if it's not a new batch.
            new_actions.push(SyncAction::Miniblock {
                params: MiniblockParams {
                    timestamp: block.timestamp,
                    virtual_blocks: block.virtual_blocks,
                },
                number: block.number,
            });
            FETCHER_METRICS.miniblock.set(block.number.0.into());
        }

        APP_METRICS.processed_txs[&TxStage::added_to_mempool()]
            .inc_by(block.transactions.len() as u64);
        new_actions.extend(block.transactions.into_iter().map(Into::into));

        // Last miniblock of the batch is a "fictive" miniblock and would be replicated locally.
        // We don't need to seal it explicitly, so we only put the seal miniblock command if it's not the last miniblock.
        if block.last_in_batch {
            new_actions.push(SyncAction::SealBatch);
        } else {
            new_actions.push(SyncAction::SealMiniblock);
        }
        self.next_miniblock += 1;
        self.prev_miniblock_hash = local_block_hash;

        new_actions
    }
}
