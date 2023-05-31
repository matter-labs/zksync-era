use vm::{vm::VmTxExecutionResult, vm_with_bootloader::BlockContextMode};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::{
    block::BlockGasCount,
    storage_writes_deduplicator::StorageWritesDeduplicator,
    tx::tx_execution_info::{ExecutionMetrics, VmExecutionLogs},
    Transaction,
};
use zksync_utils::bytecode::CompressedBytecodeInfo;

pub mod l1_batch_updates;
pub mod miniblock_updates;

pub(crate) use self::{l1_batch_updates::L1BatchUpdates, miniblock_updates::MiniblockUpdates};

/// Most of the information needed to seal the l1 batch/mini-block is contained within the VM,
/// things that are not captured there are accumulated externally.
/// `MiniblockUpdates` keeps updates for the pending mini-block.
/// `L1BatchUpdates` keeps updates for the already sealed mini-blocks of the pending L1 batch.
/// `UpdatesManager` manages the state of both of these accumulators to be consistent
/// and provides information about the pending state of the current L1 batch.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdatesManager {
    batch_timestamp: u64,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    base_fee_per_gas: u64,
    base_system_contract_hashes: BaseSystemContractsHashes,
    pub l1_batch: L1BatchUpdates,
    pub miniblock: MiniblockUpdates,
    pub storage_writes_deduplicator: StorageWritesDeduplicator,
}

impl UpdatesManager {
    pub(crate) fn new(
        block_context: &BlockContextMode,
        base_system_contract_hashes: BaseSystemContractsHashes,
    ) -> Self {
        let batch_timestamp = block_context.timestamp();
        let context = block_context.inner_block_context().context;
        Self {
            batch_timestamp,
            l1_gas_price: context.l1_gas_price,
            fair_l2_gas_price: context.fair_l2_gas_price,
            base_fee_per_gas: block_context.inner_block_context().base_fee,
            base_system_contract_hashes,
            l1_batch: L1BatchUpdates::new(),
            miniblock: MiniblockUpdates::new(batch_timestamp),
            storage_writes_deduplicator: StorageWritesDeduplicator::new(),
        }
    }

    pub(crate) fn batch_timestamp(&self) -> u64 {
        self.batch_timestamp
    }

    pub(crate) fn base_system_contract_hashes(&self) -> BaseSystemContractsHashes {
        self.base_system_contract_hashes
    }

    pub(crate) fn l1_gas_price(&self) -> u64 {
        self.l1_gas_price
    }

    pub(crate) fn fair_l2_gas_price(&self) -> u64 {
        self.fair_l2_gas_price
    }

    pub(crate) fn base_fee_per_gas(&self) -> u64 {
        self.base_fee_per_gas
    }

    pub(crate) fn extend_from_executed_transaction(
        &mut self,
        tx: &Transaction,
        tx_execution_result: VmTxExecutionResult,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        tx_l1_gas_this_tx: BlockGasCount,
        execution_metrics: ExecutionMetrics,
    ) {
        self.storage_writes_deduplicator
            .apply(&tx_execution_result.result.logs.storage_logs);
        self.miniblock.extend_from_executed_transaction(
            tx,
            tx_execution_result,
            tx_l1_gas_this_tx,
            execution_metrics,
            compressed_bytecodes,
        );
    }

    pub(crate) fn extend_from_fictive_transaction(&mut self, vm_execution_logs: VmExecutionLogs) {
        self.storage_writes_deduplicator
            .apply(&vm_execution_logs.storage_logs);
        self.miniblock
            .extend_from_fictive_transaction(vm_execution_logs);
    }

    pub(crate) fn seal_miniblock(&mut self, new_miniblock_timestamp: u64) {
        let new_miniblock_updates = MiniblockUpdates::new(new_miniblock_timestamp);
        let old_miniblock_updates = std::mem::replace(&mut self.miniblock, new_miniblock_updates);

        self.l1_batch
            .extend_from_sealed_miniblock(old_miniblock_updates);
    }

    pub(crate) fn pending_executed_transactions_len(&self) -> usize {
        self.l1_batch.executed_transactions.len() + self.miniblock.executed_transactions.len()
    }

    pub(crate) fn pending_l1_gas_count(&self) -> BlockGasCount {
        self.l1_batch.l1_gas_count + self.miniblock.l1_gas_count
    }

    pub(crate) fn pending_execution_metrics(&self) -> ExecutionMetrics {
        self.l1_batch.block_execution_metrics + self.miniblock.block_execution_metrics
    }

    pub(crate) fn pending_txs_encoding_size(&self) -> usize {
        self.l1_batch.txs_encoding_size + self.miniblock.txs_encoding_size
    }

    pub(crate) fn get_tx_by_index(&self, index: usize) -> &Transaction {
        if index < self.l1_batch.executed_transactions.len() {
            &self.l1_batch.executed_transactions[index].transaction
        } else if index < self.pending_executed_transactions_len() {
            &self.miniblock.executed_transactions[index - self.l1_batch.executed_transactions.len()]
                .transaction
        } else {
            panic!("Incorrect index provided");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_tracker::new_block_gas_count;
    use vm::vm::VmPartialExecutionResult;
    use vm::vm_with_bootloader::{BlockContext, DerivedBlockContext};
    use zksync_types::tx::tx_execution_info::{TxExecutionStatus, VmExecutionLogs};
    use zksync_types::{l2::L2Tx, Address, Nonce, H256, U256};

    #[test]
    fn apply_miniblock() {
        // Init accumulators.
        let block_context = BlockContextMode::NewBlock(
            DerivedBlockContext {
                context: BlockContext {
                    block_number: 0,
                    block_timestamp: 0,
                    l1_gas_price: 0,
                    fair_l2_gas_price: 0,
                    operator_address: Default::default(),
                },
                base_fee: 0,
            },
            0.into(),
        );
        let mut updates_manager = UpdatesManager::new(&block_context, Default::default());
        assert_eq!(updates_manager.pending_executed_transactions_len(), 0);

        // Apply tx.
        let mut tx = L2Tx::new(
            Default::default(),
            Default::default(),
            Nonce(0),
            Default::default(),
            Address::default(),
            U256::zero(),
            None,
            Default::default(),
        );
        tx.set_input(H256::random().0.to_vec(), H256::random());
        updates_manager.extend_from_executed_transaction(
            &tx.into(),
            VmTxExecutionResult {
                status: TxExecutionStatus::Success,
                result: VmPartialExecutionResult {
                    logs: VmExecutionLogs::default(),
                    revert_reason: None,
                    contracts_used: 0,
                    cycles_used: 0,
                    computational_gas_used: 0,
                },
                call_traces: vec![],
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
            vec![],
            new_block_gas_count(),
            Default::default(),
        );

        // Check that only pending state is updated.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert_eq!(updates_manager.miniblock.executed_transactions.len(), 1);
        assert_eq!(updates_manager.l1_batch.executed_transactions.len(), 0);

        // Seal miniblock.
        updates_manager.seal_miniblock(2);

        // Check that L1 batch updates are the same with the pending state
        // and miniblock updates are empty.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert_eq!(updates_manager.miniblock.executed_transactions.len(), 0);
        assert_eq!(updates_manager.l1_batch.executed_transactions.len(), 1);
    }
}
