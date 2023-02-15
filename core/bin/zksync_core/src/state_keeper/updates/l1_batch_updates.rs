use super::miniblock_updates::MiniblockUpdates;
use crate::gas_tracker::new_block_gas_count;
use zksync_types::block::BlockGasCount;
use zksync_types::priority_op_onchain_data::PriorityOpOnchainData;
use zksync_types::tx::tx_execution_info::ExecutionMetrics;
use zksync_types::{tx::TransactionExecutionResult, ExecuteTransactionCommon};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct L1BatchUpdates {
    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    pub block_execution_metrics: ExecutionMetrics,
    // how much L1 gas will it take to submit this block?
    pub l1_gas_count: BlockGasCount,
    // We keep track on the number of modified storage keys to close the block by L1 gas
    // Later on, we'll replace it with closing L2 blocks by gas.
    pub modified_storage_keys_number: usize,
}

impl L1BatchUpdates {
    pub(crate) fn new() -> Self {
        Self {
            executed_transactions: Default::default(),
            priority_ops_onchain_data: Default::default(),
            block_execution_metrics: Default::default(),
            l1_gas_count: new_block_gas_count(),
            modified_storage_keys_number: 0,
        }
    }

    pub(crate) fn extend_from_sealed_miniblock(&mut self, miniblock_updates: MiniblockUpdates) {
        for tx in miniblock_updates.executed_transactions.iter() {
            if let ExecuteTransactionCommon::L1(data) = &tx.transaction.common_data {
                let onchain_metadata = data.onchain_metadata().onchain_data;
                self.priority_ops_onchain_data.push(onchain_metadata);
            }
        }
        self.executed_transactions
            .extend(miniblock_updates.executed_transactions);

        self.modified_storage_keys_number += miniblock_updates.modified_storage_keys_number;
        self.l1_gas_count += miniblock_updates.l1_gas_count;
        self.block_execution_metrics += miniblock_updates.block_execution_metrics;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_tracker::new_block_gas_count;
    use vm::vm::{VmPartialExecutionResult, VmTxExecutionResult};
    use zksync_types::tx::tx_execution_info::TxExecutionStatus;
    use zksync_types::{l2::L2Tx, Address, Nonce, H256, U256};

    #[test]
    fn apply_miniblock_with_empty_tx() {
        let mut miniblock_accumulator = MiniblockUpdates::new(0);
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

        miniblock_accumulator.extend_from_executed_transaction(
            &tx.into(),
            VmTxExecutionResult {
                status: TxExecutionStatus::Success,
                result: VmPartialExecutionResult {
                    logs: Default::default(),
                    revert_reason: None,
                    contracts_used: 0,
                    cycles_used: 0,
                },
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
            Default::default(),
            Default::default(),
        );

        let mut l1_batch_accumulator = L1BatchUpdates::new();
        l1_batch_accumulator.extend_from_sealed_miniblock(miniblock_accumulator);

        assert_eq!(l1_batch_accumulator.executed_transactions.len(), 1);
        assert_eq!(l1_batch_accumulator.l1_gas_count, new_block_gas_count());
        assert_eq!(l1_batch_accumulator.modified_storage_keys_number, 0);
        assert_eq!(l1_batch_accumulator.priority_ops_onchain_data.len(), 0);
        assert_eq!(l1_batch_accumulator.block_execution_metrics.l2_l1_logs, 0);
        assert_eq!(
            l1_batch_accumulator
                .block_execution_metrics
                .initial_storage_writes,
            0
        );
        assert_eq!(
            l1_batch_accumulator
                .block_execution_metrics
                .repeated_storage_writes,
            0
        );
    }
}
