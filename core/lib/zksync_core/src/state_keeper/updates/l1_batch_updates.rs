use multivm::interface::FinishedL1Batch;
use zksync_types::{
    block::BlockGasCount,
    priority_op_onchain_data::PriorityOpOnchainData,
    tx::{tx_execution_info::ExecutionMetrics, TransactionExecutionResult},
    ExecuteTransactionCommon, L1BatchNumber,
};

use super::l2_block_updates::L2BlockUpdates;
use crate::gas_tracker::new_block_gas_count;

#[derive(Debug)]
pub struct L1BatchUpdates {
    pub number: L1BatchNumber,
    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    pub block_execution_metrics: ExecutionMetrics,
    // how much L1 gas will it take to submit this block?
    pub l1_gas_count: BlockGasCount,
    pub txs_encoding_size: usize,
    pub finished: Option<FinishedL1Batch>,
}

impl L1BatchUpdates {
    pub(crate) fn new(number: L1BatchNumber) -> Self {
        Self {
            number,
            executed_transactions: Default::default(),
            priority_ops_onchain_data: Default::default(),
            block_execution_metrics: Default::default(),
            l1_gas_count: new_block_gas_count(),
            txs_encoding_size: 0,
            finished: None,
        }
    }

    pub(crate) fn extend_from_sealed_l2_block(&mut self, l2_block_updates: L2BlockUpdates) {
        for tx in &l2_block_updates.executed_transactions {
            if let ExecuteTransactionCommon::L1(data) = &tx.transaction.common_data {
                let onchain_metadata = data.onchain_metadata().onchain_data;
                self.priority_ops_onchain_data.push(onchain_metadata);
            }
        }
        self.executed_transactions
            .extend(l2_block_updates.executed_transactions);

        self.l1_gas_count += l2_block_updates.l1_gas_count;
        self.block_execution_metrics += l2_block_updates.block_execution_metrics;
        self.txs_encoding_size += l2_block_updates.txs_encoding_size;
    }
}

#[cfg(test)]
mod tests {
    use multivm::vm_latest::TransactionVmExt;
    use zksync_types::{L2BlockNumber, ProtocolVersionId, H256};

    use super::*;
    use crate::{
        gas_tracker::new_block_gas_count,
        state_keeper::tests::{create_execution_result, create_transaction},
    };

    #[test]
    fn apply_l2_block_with_empty_tx() {
        let mut l2_block_accumulator = L2BlockUpdates::new(
            0,
            L2BlockNumber(0),
            H256::zero(),
            1,
            ProtocolVersionId::latest(),
        );
        let tx = create_transaction(10, 100);
        let expected_tx_size = tx.bootloader_encoding_size();

        l2_block_accumulator.extend_from_executed_transaction(
            tx,
            create_execution_result(0, []),
            BlockGasCount::default(),
            ExecutionMetrics::default(),
            vec![],
            vec![],
        );

        let mut l1_batch_accumulator = L1BatchUpdates::new(L1BatchNumber(1));
        l1_batch_accumulator.extend_from_sealed_l2_block(l2_block_accumulator);

        assert_eq!(l1_batch_accumulator.executed_transactions.len(), 1);
        assert_eq!(l1_batch_accumulator.l1_gas_count, new_block_gas_count());
        assert_eq!(l1_batch_accumulator.priority_ops_onchain_data.len(), 0);
        assert_eq!(
            l1_batch_accumulator.block_execution_metrics.l2_to_l1_logs,
            0
        );
        assert_eq!(l1_batch_accumulator.txs_encoding_size, expected_tx_size);
    }
}
