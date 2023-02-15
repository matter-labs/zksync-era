use vm::{vm::VmTxExecutionResult, vm_with_bootloader::BlockContextMode};
use zksync_types::block::BlockGasCount;
use zksync_types::tx::ExecutionMetrics;
use zksync_types::Transaction;

mod l1_batch_updates;
mod miniblock_updates;

pub(crate) use self::{l1_batch_updates::L1BatchUpdates, miniblock_updates::MiniblockUpdates};

/// Most of the information needed to seal the l1 batch/mini-block is contained within the VM,
/// things that are not captured there are accumulated externally.
/// `MiniblockUpdates` keeps updates for the pending mini-block.
/// `L1BatchUpdates` keeps updates for the already sealed mini-blocks of the pending L1 batch.
/// `UpdatesManager` manages the state of both of these accumulators to be consistent
/// and provides information about the pending state of the current L1 batch.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UpdatesManager {
    batch_timestamp: u64,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    base_fee_per_gas: u64,
    pub l1_batch: L1BatchUpdates,
    pub miniblock: MiniblockUpdates,
}

impl UpdatesManager {
    pub(crate) fn new(block_context: &BlockContextMode) -> Self {
        let batch_timestamp = block_context.timestamp();
        let context = block_context.inner_block_context().context;
        Self {
            batch_timestamp,
            l1_gas_price: context.l1_gas_price,
            fair_l2_gas_price: context.fair_l2_gas_price,
            base_fee_per_gas: block_context.inner_block_context().base_fee,
            l1_batch: L1BatchUpdates::new(),
            miniblock: MiniblockUpdates::new(batch_timestamp),
        }
    }

    pub(crate) fn batch_timestamp(&self) -> u64 {
        self.batch_timestamp
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
        tx_l1_gas_this_tx: BlockGasCount,
        execution_metrics: ExecutionMetrics,
    ) {
        self.miniblock.extend_from_executed_transaction(
            tx,
            tx_execution_result,
            tx_l1_gas_this_tx,
            execution_metrics,
        )
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
        let mut updates_manager = UpdatesManager::new(&block_context);
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
                },
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
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
