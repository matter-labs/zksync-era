use anyhow::Context;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_state_keeper::io::{
    common::{FetcherCursor, IoCursor},
    L1BatchParams, L2BlockParams,
};
use zksync_types::{
    api::en::SyncBlock, block::L2BlockHasher, commitment::PubdataParams, fee_model::BatchFeeInput,
    helpers::unix_timestamp_ms, Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId, H256,
};

use super::{
    metrics::{L1BatchStage, FETCHER_METRICS},
    sync_action::SyncAction,
};

/// Same as [`zksync_types::Transaction`], just with additional guarantees that the "received at" timestamp was set locally.
///
/// We cannot transfer `Transaction`s without these timestamps, because this would break backward compatibility.
#[derive(Debug, Clone)]
pub struct FetchedTransaction(zksync_types::Transaction);

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
pub struct FetchedBlock {
    pub number: L2BlockNumber,
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
    pub pubdata_params: PubdataParams,
    pub pubdata_limit: Option<u64>,
}

impl FetchedBlock {
    fn compute_hash(&self, prev_l2_block_hash: H256) -> H256 {
        let mut hasher = L2BlockHasher::new(self.number, self.timestamp, prev_l2_block_hash);
        for tx in &self.transactions {
            hasher.push_tx_hash(tx.hash());
        }
        hasher.finalize(self.protocol_version)
    }

    pub fn into_actions(self, cursor: FetcherCursor) -> Vec<SyncAction> {
        assert_eq!(self.number, cursor.next_l2_block);
        let local_block_hash = self.compute_hash(cursor.prev_l2_block_hash);
        if let Some(reference_hash) = self.reference_hash {
            if local_block_hash != reference_hash {
                // This is a warning, not an assertion because hash mismatch may occur after a reorg.
                // Indeed, `self.prev_l2_block_hash` may differ from the hash of the updated previous L2 block.
                tracing::warn!(
                    "Mismatch between the locally computed and received L2 block hash for {self:?}; \
                     local_block_hash = {local_block_hash:?}, prev_l2_block_hash = {:?}",
                    cursor.prev_l2_block_hash
                );
            }
        }
        assert_eq!(
            self.l1_batch_number, cursor.current_l1_batch,
            "Unexpected batch number in the next received L2 block"
        );

        let mut new_actions = Vec::new();
        if !cursor.is_current_batch_init {
            tracing::info!(
                "New L1 batch: {}. Timestamp: {}",
                self.l1_batch_number,
                self.timestamp
            );

            new_actions.push(SyncAction::OpenBatch {
                params: L1BatchParams {
                    protocol_version: self.protocol_version,
                    validation_computational_gas_limit: super::VALIDATION_COMPUTATIONAL_GAS_LIMIT,
                    operator_address: self.operator_address,
                    fee_input: BatchFeeInput::for_protocol_version(
                        self.protocol_version,
                        self.l2_fair_gas_price,
                        self.fair_pubdata_price,
                        self.l1_gas_price,
                    ),
                    // It's ok that we lose info about millis since it's only used for sealing criteria.
                    first_l2_block: L2BlockParams::with_custom_virtual_block_count(
                        self.timestamp * 1000,
                        self.virtual_blocks,
                    ),
                    pubdata_params: self.pubdata_params,
                    pubdata_limit: self.pubdata_limit,
                },
                number: self.l1_batch_number,
                first_l2_block_number: self.number,
            });
            FETCHER_METRICS.l1_batch[&L1BatchStage::Open].set(self.l1_batch_number.0.into());
        } else {
            // New batch implicitly means a new L2 block, so we only need to push the L2 block action
            // if it's not a new batch.
            new_actions.push(SyncAction::L2Block {
                // It's ok that we lose info about millis since it's only used for sealing criteria.
                params: L2BlockParams::with_custom_virtual_block_count(
                    self.timestamp * 1000,
                    self.virtual_blocks,
                ),
                number: self.number,
            });
            FETCHER_METRICS.miniblock.set(self.number.0.into());
        }

        APP_METRICS.processed_txs[&TxStage::added_to_mempool()]
            .inc_by(self.transactions.len() as u64);
        new_actions.extend(self.transactions.into_iter().map(Into::into));

        // Last L2 block of the batch is a "fictive" L2 block and would be replicated locally.
        // We don't need to seal it explicitly, so we only put the seal L2 block command if it's not the last L2 block.
        if self.last_in_batch {
            new_actions.push(SyncAction::SealBatch);
        } else {
            new_actions.push(SyncAction::SealL2Block);
        }

        new_actions
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
                "Only last L2 block of the batch can be empty"
            ));
        }

        let pubdata_params = if block.protocol_version.is_pre_gateway() {
            block.pubdata_params.unwrap_or_default()
        } else {
            block
                .pubdata_params
                .context("Missing `pubdata_params` for post-gateway payload")?
        };

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
            pubdata_params,
            pubdata_limit: block.pubdata_limit,
        })
    }
}
