use multivm::{
    interface::{FinishedL1Batch, L1BatchEnv, SystemEnv, VmExecutionResultAndLogs},
    utils::get_batch_base_fee,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_state::StorageViewCache;
use zksync_types::{
    block::BlockGasCount, fee_model::BatchFeeInput,
    storage_writes_deduplicator::StorageWritesDeduplicator,
    tx::tx_execution_info::ExecutionMetrics, vm_trace::Call, Address, L1BatchNumber, L2BlockNumber,
    ProtocolVersionId, Transaction,
};
use zksync_utils::bytecode::CompressedBytecodeInfo;

pub(crate) use self::{l1_batch_updates::L1BatchUpdates, l2_block_updates::L2BlockUpdates};
use super::{
    io::{IoCursor, L2BlockParams},
    metrics::{BATCH_TIP_METRICS, UPDATES_MANAGER_METRICS},
};
use crate::types::ExecutionMetricsForCriteria;

pub mod l1_batch_updates;
pub mod l2_block_updates;

/// Most of the information needed to seal the l1 batch / L2 block is contained within the VM,
/// things that are not captured there are accumulated externally.
/// `L2BlockUpdates` keeps updates for the pending L2 block.
/// `L1BatchUpdates` keeps updates for the already sealed L2 blocks of the pending L1 batch.
/// `UpdatesManager` manages the state of both of these accumulators to be consistent
/// and provides information about the pending state of the current L1 batch.
#[derive(Debug)]
pub struct UpdatesManager {
    batch_timestamp: u64,
    fee_account_address: Address,
    batch_fee_input: BatchFeeInput,
    base_fee_per_gas: u64,
    base_system_contract_hashes: BaseSystemContractsHashes,
    protocol_version: ProtocolVersionId,
    pub storage_view_cache: StorageViewCache,
    pub l1_batch: L1BatchUpdates,
    pub l2_block: L2BlockUpdates,
    pub storage_writes_deduplicator: StorageWritesDeduplicator,
}

impl UpdatesManager {
    pub fn new(l1_batch_env: &L1BatchEnv, system_env: &SystemEnv) -> Self {
        let protocol_version = system_env.version;
        Self {
            batch_timestamp: l1_batch_env.timestamp,
            fee_account_address: l1_batch_env.fee_account,
            batch_fee_input: l1_batch_env.fee_input,
            base_fee_per_gas: get_batch_base_fee(l1_batch_env, protocol_version.into()),
            protocol_version,
            base_system_contract_hashes: system_env.base_system_smart_contracts.hashes(),
            l1_batch: L1BatchUpdates::new(l1_batch_env.number),
            l2_block: L2BlockUpdates::new(
                l1_batch_env.first_l2_block.timestamp,
                L2BlockNumber(l1_batch_env.first_l2_block.number),
                l1_batch_env.first_l2_block.prev_block_hash,
                l1_batch_env.first_l2_block.max_virtual_blocks_to_create,
                protocol_version,
            ),
            storage_writes_deduplicator: StorageWritesDeduplicator::new(),
            storage_view_cache: StorageViewCache::default(),
        }
    }

    pub(crate) fn batch_timestamp(&self) -> u64 {
        self.batch_timestamp
    }

    pub(crate) fn base_system_contract_hashes(&self) -> BaseSystemContractsHashes {
        self.base_system_contract_hashes
    }

    pub(crate) fn io_cursor(&self) -> IoCursor {
        IoCursor {
            next_l2_block: self.l2_block.number + 1,
            prev_l2_block_hash: self.l2_block.get_l2_block_hash(),
            prev_l2_block_timestamp: self.l2_block.timestamp,
            l1_batch: self.l1_batch.number,
        }
    }

    pub(crate) fn seal_l2_block_command(
        &self,
        l2_shared_bridge_addr: Address,
        pre_insert_txs: bool,
    ) -> L2BlockSealCommand {
        L2BlockSealCommand {
            l1_batch_number: self.l1_batch.number,
            l2_block: self.l2_block.clone(),
            first_tx_index: self.l1_batch.executed_transactions.len(),
            fee_account_address: self.fee_account_address,
            fee_input: self.batch_fee_input,
            base_fee_per_gas: self.base_fee_per_gas,
            base_system_contracts_hashes: self.base_system_contract_hashes,
            protocol_version: Some(self.protocol_version),
            l2_shared_bridge_addr,
            pre_insert_txs,
        }
    }

    pub(crate) fn protocol_version(&self) -> ProtocolVersionId {
        self.protocol_version
    }

    pub fn extend_from_executed_transaction(
        &mut self,
        tx: Transaction,
        tx_execution_result: VmExecutionResultAndLogs,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        tx_l1_gas_this_tx: BlockGasCount,
        execution_metrics: ExecutionMetrics,
        call_traces: Vec<Call>,
    ) {
        let latency = UPDATES_MANAGER_METRICS
            .extend_from_executed_transaction
            .start();
        self.storage_writes_deduplicator
            .apply(&tx_execution_result.logs.storage_logs);
        self.l2_block.extend_from_executed_transaction(
            tx,
            tx_execution_result,
            tx_l1_gas_this_tx,
            execution_metrics,
            compressed_bytecodes,
            call_traces,
        );
        latency.observe();
    }

    pub fn finish_batch(&mut self, finished_batch: FinishedL1Batch) {
        let latency = UPDATES_MANAGER_METRICS.finish_batch.start();
        assert!(
            self.l1_batch.finished.is_none(),
            "Cannot finish already finished batch"
        );

        let result = &finished_batch.block_tip_execution_result;
        let batch_tip_metrics = ExecutionMetricsForCriteria::new(None, result);

        let before = self.storage_writes_deduplicator.metrics();
        self.storage_writes_deduplicator
            .apply(&result.logs.storage_logs);
        let after = self.storage_writes_deduplicator.metrics();
        BATCH_TIP_METRICS.observe_writes_metrics(&before, &after, self.protocol_version());

        self.l2_block.extend_from_fictive_transaction(
            result.clone(),
            batch_tip_metrics.l1_gas,
            batch_tip_metrics.execution_metrics,
        );
        self.l1_batch.finished = Some(finished_batch);

        latency.observe();
    }

    pub fn update_storage_view_cache(&mut self, storage_view_cache: StorageViewCache) {
        self.storage_view_cache = storage_view_cache;
    }

    /// Pushes a new L2 block with the specified timestamp into this manager. The previously
    /// held L2 block is considered sealed and is used to extend the L1 batch data.
    pub fn push_l2_block(&mut self, l2_block_params: L2BlockParams) {
        let new_l2_block_updates = L2BlockUpdates::new(
            l2_block_params.timestamp,
            self.l2_block.number + 1,
            self.l2_block.get_l2_block_hash(),
            l2_block_params.virtual_blocks,
            self.protocol_version,
        );
        let old_l2_block_updates = std::mem::replace(&mut self.l2_block, new_l2_block_updates);
        self.l1_batch
            .extend_from_sealed_l2_block(old_l2_block_updates);
    }

    pub(crate) fn pending_executed_transactions_len(&self) -> usize {
        self.l1_batch.executed_transactions.len() + self.l2_block.executed_transactions.len()
    }

    pub(crate) fn pending_l1_gas_count(&self) -> BlockGasCount {
        self.l1_batch.l1_gas_count + self.l2_block.l1_gas_count
    }

    pub(crate) fn pending_execution_metrics(&self) -> ExecutionMetrics {
        self.l1_batch.block_execution_metrics + self.l2_block.block_execution_metrics
    }

    pub(crate) fn pending_txs_encoding_size(&self) -> usize {
        self.l1_batch.txs_encoding_size + self.l2_block.txs_encoding_size
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
    pub l2_shared_bridge_addr: Address,
    /// Whether transactions should be pre-inserted to DB.
    /// Should be set to `true` for EN's IO as EN doesn't store transactions in DB
    /// before they are included into L2 blocks.
    pub pre_insert_txs: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tests::{create_execution_result, create_transaction, create_updates_manager},
        utils::new_block_gas_count,
    };

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
            vec![],
            new_block_gas_count(),
            ExecutionMetrics::default(),
            vec![],
        );

        // Check that only pending state is updated.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert_eq!(updates_manager.l2_block.executed_transactions.len(), 1);
        assert_eq!(updates_manager.l1_batch.executed_transactions.len(), 0);

        // Seal an L2 block.
        updates_manager.push_l2_block(L2BlockParams {
            timestamp: 2,
            virtual_blocks: 1,
        });

        // Check that L1 batch updates are the same with the pending state
        // and L2 block updates are empty.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert_eq!(updates_manager.l2_block.executed_transactions.len(), 0);
        assert_eq!(updates_manager.l1_batch.executed_transactions.len(), 1);
    }
}
