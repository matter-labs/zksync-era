use std::collections::VecDeque;

use zksync_contracts::BaseSystemContractsHashes;
use zksync_multivm::{
    interface::{Call, FinishedL1Batch, VmExecutionMetrics, VmExecutionResultAndLogs},
    utils::{
        get_batch_base_fee, get_bootloader_max_interop_roots_in_batch, get_max_batch_gas_limit,
        get_max_gas_per_pubdata_byte, StorageWritesDeduplicator,
    },
};
use zksync_types::{
    block::{build_bloom, L2BlockHeader},
    commitment::PubdataParams,
    fee_model::BatchFeeInput,
    Address, BloomInput, L1BatchNumber, L2BlockNumber, ProtocolVersionId, Transaction, H256,
};

pub(crate) use self::{committed_updates::CommittedUpdates, l2_block_updates::L2BlockUpdates};
use super::{
    io::{BatchInitParams, IoCursor, L2BlockParams},
    metrics::{BATCH_TIP_METRICS, UPDATES_MANAGER_METRICS},
};
use crate::{
    metrics::{L2BlockSealStage, L2_BLOCK_METRICS},
    updates::l2_block_updates::RollingTxHashUpdates,
};

pub mod committed_updates;
pub mod l2_block_updates;

/// Most of the information needed to seal the l1 batch / L2 block is contained within the VM.
///
/// Things that are not captured there are accumulated externally.
/// `L2BlockUpdates` keeps updates for the pending L2 block.
/// `L1BatchUpdates` keeps updates for the already sealed L2 blocks of the pending L1 batch.
/// `UpdatesManager` manages the state of both of these accumulators to be consistent
/// and provides information about the pending state of the current L1 batch.
#[derive(Debug)]
pub struct UpdatesManager {
    // immutable
    l1_batch_number: L1BatchNumber,
    l1_batch_timestamp: u64,
    fee_account_address: Address,
    batch_fee_input: BatchFeeInput,
    base_fee_per_gas: u64,
    base_system_contract_hashes: BaseSystemContractsHashes,
    protocol_version: ProtocolVersionId,
    rolling_tx_hash_updates: RollingTxHashUpdates,
    pubdata_params: PubdataParams,
    pubdata_limit: Option<u64>,
    previous_batch_protocol_version: ProtocolVersionId,
    previous_batch_timestamp: u64,
    sync_block_data_and_header_persistence: bool,

    // committed state
    committed_updates: CommittedUpdates,
    last_committed_l2_block_number: L2BlockNumber,
    last_committed_l2_block_timestamp: Option<u64>,
    last_committed_l2_block_hash: H256,

    // pending state
    pending_l2_blocks: VecDeque<L2BlockUpdates>,
    storage_writes_deduplicator: StorageWritesDeduplicator,
    next_l2_block_params: Option<L2BlockParams>,
}

impl UpdatesManager {
    pub(crate) fn new(
        batch_init_params: &BatchInitParams,
        previous_batch_protocol_version: ProtocolVersionId,
        previous_batch_timestamp: u64,
        previous_block_timestamp: Option<u64>,
        sync_block_data_and_header_persistence: bool,
    ) -> Self {
        let protocol_version = batch_init_params.system_env.version;
        let mut storage_writes_deduplicator = StorageWritesDeduplicator::new();
        storage_writes_deduplicator.make_snapshot();
        Self {
            l1_batch_number: batch_init_params.l1_batch_env.number,
            l1_batch_timestamp: batch_init_params.l1_batch_env.timestamp,
            fee_account_address: batch_init_params.l1_batch_env.fee_account,
            batch_fee_input: batch_init_params.l1_batch_env.fee_input,
            base_fee_per_gas: get_batch_base_fee(
                &batch_init_params.l1_batch_env,
                protocol_version.into(),
            ),
            base_system_contract_hashes: batch_init_params
                .system_env
                .base_system_smart_contracts
                .hashes(),
            protocol_version,
            pubdata_params: batch_init_params.pubdata_params,
            pubdata_limit: batch_init_params.pubdata_limit,
            previous_batch_protocol_version,
            previous_batch_timestamp,
            sync_block_data_and_header_persistence,
            committed_updates: CommittedUpdates::new(),
            last_committed_l2_block_number: L2BlockNumber(
                batch_init_params.l1_batch_env.first_l2_block.number,
            ) - 1,
            last_committed_l2_block_timestamp: previous_block_timestamp,
            last_committed_l2_block_hash: batch_init_params
                .l1_batch_env
                .first_l2_block
                .prev_block_hash,
            pending_l2_blocks: VecDeque::from([L2BlockUpdates::new(
                batch_init_params.timestamp_ms,
                L2BlockNumber(batch_init_params.l1_batch_env.first_l2_block.number),
                batch_init_params
                    .l1_batch_env
                    .first_l2_block
                    .prev_block_hash,
                batch_init_params
                    .l1_batch_env
                    .first_l2_block
                    .max_virtual_blocks_to_create,
                protocol_version,
                batch_init_params
                    .l1_batch_env
                    .first_l2_block
                    .interop_roots
                    .clone(),
            )]),
            rolling_tx_hash_updates: RollingTxHashUpdates {
                rolling_hash: H256::zero(),
            },
            storage_writes_deduplicator,
            next_l2_block_params: None,
        }
    }

    pub(crate) fn next_l2_block_timestamp_ms_mut(&mut self) -> Option<&mut u64> {
        self.next_l2_block_params
            .as_mut()
            .map(|params| params.timestamp_ms_mut())
    }

    pub(crate) fn get_next_or_current_l2_block_timestamp(&mut self) -> u64 {
        if let Some(next_l2_block_params) = &self.next_l2_block_params {
            next_l2_block_params.timestamp()
        } else {
            self.last_pending_l2_block().timestamp()
        }
    }

    pub(crate) fn has_next_block_params(&self) -> bool {
        self.next_l2_block_params.is_some()
    }

    pub(crate) fn io_cursor(&self) -> IoCursor {
        if let Some(last) = self.pending_l2_blocks.back() {
            IoCursor {
                next_l2_block: last.number + 1,
                prev_l2_block_hash: last.get_l2_block_hash(),
                prev_l2_block_timestamp: last.timestamp(),
                l1_batch: self.l1_batch_number,
                prev_l1_batch_timestamp: self.previous_batch_timestamp(),
            }
        } else {
            IoCursor {
                next_l2_block: self.last_committed_l2_block_number + 1,
                prev_l2_block_hash: self.last_committed_l2_block_hash,
                prev_l2_block_timestamp: self.last_committed_l2_block_timestamp.unwrap(),
                l1_batch: self.l1_batch_number,
                prev_l1_batch_timestamp: self.previous_batch_timestamp(),
            }
        }
    }

    pub(crate) fn seal_l2_block_command(
        &self,
        l2_legacy_shared_bridge_addr: Option<Address>,
        pre_insert_data: bool,
    ) -> L2BlockSealCommand {
        let l2_block = self.last_pending_l2_block().clone();
        let tx_count_in_last_block = l2_block.executed_transactions.len();
        L2BlockSealCommand {
            l1_batch_number: self.l1_batch_number(),
            l2_block,
            first_tx_index: self.pending_executed_transactions_len() - tx_count_in_last_block,
            fee_account_address: self.fee_account_address,
            fee_input: self.batch_fee_input,
            base_fee_per_gas: self.base_fee_per_gas,
            base_system_contracts_hashes: self.base_system_contract_hashes,
            protocol_version: Some(self.protocol_version),
            l2_legacy_shared_bridge_addr,
            pre_insert_data,
            pubdata_params: self.pubdata_params,
            insert_header: self.sync_block_data_and_header_persistence
                || (tx_count_in_last_block == 0),
            rolling_txs_hash: self.rolling_tx_hash_updates.rolling_hash,
        }
    }

    pub fn extend_from_executed_transaction(
        &mut self,
        tx: Transaction,
        tx_execution_result: VmExecutionResultAndLogs,
        execution_metrics: VmExecutionMetrics,
        call_traces: Vec<Call>,
    ) {
        let latency = UPDATES_MANAGER_METRICS
            .extend_from_executed_transaction
            .start();
        self.storage_writes_deduplicator
            .apply(&tx_execution_result.logs.storage_logs);

        self.rolling_tx_hash_updates
            .append_rolling_hash(tx.hash(), !tx_execution_result.result.is_failed());
        self.last_pending_l2_block_mut()
            .extend_from_executed_transaction(
                tx,
                tx_execution_result,
                execution_metrics,
                call_traces,
            );
        latency.observe();
    }

    pub fn finish_batch(&mut self, finished_batch: FinishedL1Batch) {
        let latency = UPDATES_MANAGER_METRICS.finish_batch.start();
        assert!(
            self.committed_updates.finished.is_none(),
            "Cannot finish already finished batch"
        );

        let result = &finished_batch.block_tip_execution_result;
        let batch_tip_execution_metrics = result.get_execution_metrics();

        let before = self.storage_writes_deduplicator.metrics();
        self.storage_writes_deduplicator
            .apply(&result.logs.storage_logs);
        let after = self.storage_writes_deduplicator.metrics();
        BATCH_TIP_METRICS.observe_writes_metrics(&before, &after, self.protocol_version());

        self.last_pending_l2_block_mut()
            .extend_from_fictive_transaction(result.clone(), batch_tip_execution_metrics);
        self.committed_updates.finished = Some(finished_batch);

        latency.observe();
    }

    /// Pushes a new L2 block with the specified timestamp into this manager.
    pub fn push_l2_block(&mut self) {
        let next_l2_block_params = self
            .next_l2_block_params
            .take()
            .expect("next l2 block params cannot be empty");

        let cursor = self.io_cursor();
        let new_l2_block_updates = L2BlockUpdates::new(
            next_l2_block_params.timestamp_ms(),
            cursor.next_l2_block,
            cursor.prev_l2_block_hash,
            next_l2_block_params.virtual_blocks(),
            self.protocol_version,
            next_l2_block_params.interop_roots().to_vec(),
        );
        self.pending_l2_blocks.push_back(new_l2_block_updates);
        self.storage_writes_deduplicator.make_snapshot();
    }

    pub fn set_next_l2_block_params(&mut self, mut l2_block_params: L2BlockParams) {
        assert!(
            self.next_l2_block_params.is_none(),
            "next_l2_block_params cannot be set twice"
        );
        // We need to filter already applied interop roots, and take up to the batch limit set by the bootloader.
        // Because we seal L2 blocks in async manner, it's possible that database returns already applied interop roots.
        let limit = get_bootloader_max_interop_roots_in_batch(self.protocol_version.into());
        let new_interop_roots: Vec<_> = l2_block_params
            .interop_roots()
            .iter()
            .filter(|root| {
                !self.committed_updates.interop_roots.contains(root)
                    && self
                        .pending_l2_blocks
                        .iter()
                        .all(|block| !block.interop_roots.contains(root))
            })
            .take(limit.saturating_sub(self.pending_interop_roots_len()))
            .cloned()
            .collect();

        l2_block_params.set_interop_roots(new_interop_roots);
        self.next_l2_block_params = Some(l2_block_params);
    }

    pub fn reset_next_l2_block_params(&mut self) {
        assert!(self.next_l2_block_params.is_some());
        self.next_l2_block_params = None;
    }

    pub(crate) fn pending_executed_transactions_len(&self) -> usize {
        self.committed_updates.executed_transaction_hashes.len()
            + self
                .pending_l2_blocks
                .iter()
                .map(|b| b.executed_transactions.len())
                .sum::<usize>()
    }

    pub(crate) fn pending_l1_transactions_len(&self) -> usize {
        self.committed_updates.l1_tx_count
            + self
                .pending_l2_blocks
                .iter()
                .map(|b| b.l1_tx_count)
                .sum::<usize>()
    }

    pub(crate) fn pending_interop_roots_len(&self) -> usize {
        self.committed_updates().interop_roots.len()
            + self
                .pending_l2_blocks
                .iter()
                .map(|b| b.interop_roots.len())
                .sum::<usize>()
    }

    pub(crate) fn pending_execution_metrics(&self) -> VmExecutionMetrics {
        self.committed_updates.block_execution_metrics
            + self
                .pending_l2_blocks
                .iter()
                .fold(VmExecutionMetrics::default(), |sum, b| {
                    sum + b.block_execution_metrics
                })
    }

    pub(crate) fn pending_txs_encoding_size(&self) -> usize {
        self.committed_updates.txs_encoding_size
            + self
                .pending_l2_blocks
                .iter()
                .map(|b| b.txs_encoding_size)
                .sum::<usize>()
    }

    pub(crate) fn header_for_first_pending_block(&self) -> L2BlockHeader {
        let block = self.first_pending_l2_block();
        let progress = L2_BLOCK_METRICS.start(
            L2BlockSealStage::CalculateLogsBloom,
            !block.executed_transactions.is_empty(),
        );
        let iter = block.events.iter().flat_map(|event| {
            event
                .indexed_topics
                .iter()
                .map(|topic| BloomInput::Raw(topic.as_bytes()))
                .chain([BloomInput::Raw(event.address.as_bytes())])
        });
        let logs_bloom = build_bloom(iter);
        progress.observe(Some(block.events.len()));
        let definite_vm_version = self.protocol_version().into();
        L2BlockHeader {
            number: block.number,
            timestamp: block.timestamp(),
            hash: block.get_l2_block_hash(),
            l1_tx_count: block.l1_tx_count as u16,
            l2_tx_count: (block.executed_transactions.len() - block.l1_tx_count) as u16,
            fee_account_address: self.fee_account_address,
            base_fee_per_gas: self.base_fee_per_gas,
            batch_fee_input: self.batch_fee_input,
            base_system_contracts_hashes: self.base_system_contract_hashes(),
            protocol_version: Some(self.protocol_version()),
            gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(definite_vm_version),
            virtual_blocks: block.virtual_blocks,
            gas_limit: get_max_batch_gas_limit(definite_vm_version),
            logs_bloom,
            pubdata_params: self.pubdata_params,
            rolling_txs_hash: Some(self.rolling_tx_hash_updates.rolling_hash),
        }
    }

    pub fn last_pending_l2_block(&self) -> &L2BlockUpdates {
        self.pending_l2_blocks.back().unwrap()
    }

    pub fn last_pending_l2_block_checked(&self) -> Option<&L2BlockUpdates> {
        self.pending_l2_blocks.back()
    }

    pub fn last_pending_l2_block_mut(&mut self) -> &mut L2BlockUpdates {
        self.pending_l2_blocks.back_mut().unwrap()
    }

    pub fn next_l2_block_number(&self) -> L2BlockNumber {
        if let Some(b) = self.pending_l2_blocks.back() {
            b.number + 1
        } else {
            self.last_committed_l2_block_number + 1
        }
    }

    pub fn is_in_first_pending_block_state(&self) -> bool {
        self.committed_updates
            .executed_transaction_hashes
            .is_empty()
            && self.pending_l2_blocks.len() == 1
    }

    pub fn number_of_pending_blocks(&self) -> usize {
        self.pending_l2_blocks.len()
    }

    pub fn first_pending_l2_block(&self) -> &L2BlockUpdates {
        self.pending_l2_blocks.front().unwrap()
    }

    pub fn pop_last_pending_block(&mut self) {
        self.pending_l2_blocks.pop_back();
        self.storage_writes_deduplicator
            .rollback_to_latest_snapshot_popping();
    }

    pub fn commit_pending_block(&mut self) {
        let block = self.pending_l2_blocks.pop_front().unwrap();
        self.last_committed_l2_block_number = block.number;
        self.last_committed_l2_block_hash = block.get_l2_block_hash();
        self.last_committed_l2_block_timestamp = Some(block.timestamp());
        self.committed_updates
            .interop_roots
            .extend(block.interop_roots.iter().cloned());
        self.committed_updates.extend_from_sealed_l2_block(block);
        self.storage_writes_deduplicator
            .pop_front_snapshot_no_rollback();
    }
}

// Simple getters
impl UpdatesManager {
    pub fn protocol_version(&self) -> ProtocolVersionId {
        self.protocol_version
    }

    pub fn previous_batch_protocol_version(&self) -> ProtocolVersionId {
        self.previous_batch_protocol_version
    }

    pub fn previous_batch_timestamp(&self) -> u64 {
        self.previous_batch_timestamp
    }

    pub fn l1_batch_number(&self) -> L1BatchNumber {
        self.l1_batch_number
    }

    pub fn l1_batch_timestamp(&self) -> u64 {
        self.l1_batch_timestamp
    }

    pub fn base_system_contract_hashes(&self) -> BaseSystemContractsHashes {
        self.base_system_contract_hashes
    }

    pub fn batch_fee_input(&self) -> BatchFeeInput {
        self.batch_fee_input
    }

    pub fn fee_account_address(&self) -> Address {
        self.fee_account_address
    }

    pub fn committed_updates(&self) -> &CommittedUpdates {
        &self.committed_updates
    }

    pub fn storage_writes_deduplicator(&self) -> &StorageWritesDeduplicator {
        &self.storage_writes_deduplicator
    }

    pub fn storage_writes_deduplicator_mut(&mut self) -> &mut StorageWritesDeduplicator {
        &mut self.storage_writes_deduplicator
    }

    pub fn sync_block_data_and_header_persistence(&self) -> bool {
        self.sync_block_data_and_header_persistence
    }

    pub fn pubdata_limit(&self) -> Option<u64> {
        self.pubdata_limit
    }
}

/// Command to seal an L2 block containing all necessary data for it.
#[derive(Debug)]
pub struct L2BlockSealCommand {
    pub l1_batch_number: L1BatchNumber,
    pub l2_block: L2BlockUpdates,
    pub first_tx_index: usize,
    pub fee_account_address: Address,
    pub fee_input: BatchFeeInput,
    pub base_fee_per_gas: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub protocol_version: Option<ProtocolVersionId>,
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    /// Whether transactions or interop roots should be pre-inserted to DB.
    /// Should be set to `true` for EN's IO as EN doesn't store transactions and interop roots in DB
    /// before they are included into L2 blocks.
    pub pre_insert_data: bool,
    pub pubdata_params: PubdataParams,
    pub insert_header: bool,
    pub rolling_txs_hash: H256,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{create_execution_result, create_transaction, create_updates_manager};

    #[test]
    fn apply_l2_block() {
        // Init accumulators.
        let mut updates_manager = create_updates_manager();
        assert_eq!(updates_manager.pending_executed_transactions_len(), 0);

        // Apply tx.
        let tx = create_transaction(10, 100);
        updates_manager.extend_from_executed_transaction(
            tx,
            create_execution_result([]),
            VmExecutionMetrics::default(),
            vec![],
        );

        // Check that only pending state is updated.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert_eq!(
            updates_manager
                .last_pending_l2_block()
                .executed_transactions
                .len(),
            1
        );
        assert_eq!(
            updates_manager
                .committed_updates
                .executed_transaction_hashes
                .len(),
            0
        );

        // Seal an L2 block.
        updates_manager.commit_pending_block();

        // Check that L1 batch updates are the same with the pending state
        // and L2 block updates are empty.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert!(updates_manager.last_pending_l2_block_checked().is_none());
        assert_eq!(
            updates_manager
                .committed_updates
                .executed_transaction_hashes
                .len(),
            1
        );
    }
}
