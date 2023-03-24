use super::miniblock_updates::MiniblockUpdates;
use crate::gas_tracker::new_block_gas_count;
use zksync_types::block::BlockGasCount;
use zksync_types::priority_op_onchain_data::PriorityOpOnchainData;
use zksync_types::tx::tx_execution_info::ExecutionMetrics;
use zksync_types::{tx::TransactionExecutionResult, ExecuteTransactionCommon};

#[derive(Debug, Clone, PartialEq)]
pub struct L1BatchUpdates {
    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    pub block_execution_metrics: ExecutionMetrics,
    // how much L1 gas will it take to submit this block?
    pub l1_gas_count: BlockGasCount,
    pub txs_encoding_size: usize,
}

impl L1BatchUpdates {
    pub(crate) fn new() -> Self {
        Self {
            executed_transactions: Default::default(),
            priority_ops_onchain_data: Default::default(),
            block_execution_metrics: Default::default(),
            l1_gas_count: new_block_gas_count(),
            txs_encoding_size: 0,
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

        self.l1_gas_count += miniblock_updates.l1_gas_count;
        self.block_execution_metrics += miniblock_updates.block_execution_metrics;
        self.txs_encoding_size += miniblock_updates.txs_encoding_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_tracker::new_block_gas_count;
    use vm::transaction_data::TransactionData;
    use vm::vm::{VmPartialExecutionResult, VmTxExecutionResult};
    use zksync_types::tx::tx_execution_info::TxExecutionStatus;
    use zksync_types::{l2::L2Tx, Address, Nonce, Transaction, H256, U256};

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
        let tx: Transaction = tx.into();

        miniblock_accumulator.extend_from_executed_transaction(
            &tx,
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
            Default::default(),
        );

        let mut l1_batch_accumulator = L1BatchUpdates::new();
        l1_batch_accumulator.extend_from_sealed_miniblock(miniblock_accumulator);

        assert_eq!(l1_batch_accumulator.executed_transactions.len(), 1);
        assert_eq!(l1_batch_accumulator.l1_gas_count, new_block_gas_count());
        assert_eq!(l1_batch_accumulator.priority_ops_onchain_data.len(), 0);
        assert_eq!(l1_batch_accumulator.block_execution_metrics.l2_l1_logs, 0);

        let tx_data: TransactionData = tx.into();
        assert_eq!(
            l1_batch_accumulator.txs_encoding_size,
            tx_data.into_tokens().len()
        );
    }
}
