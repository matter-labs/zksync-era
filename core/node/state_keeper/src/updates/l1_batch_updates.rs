use zksync_multivm::interface::{FinishedL1Batch, VmExecutionMetrics};
use zksync_types::{
    priority_op_onchain_data::PriorityOpOnchainData, ExecuteTransactionCommon, L1BatchNumber, H256,
};

use crate::updates::l2_block_updates::L2BlockUpdates;

#[derive(Debug)]
pub struct L1BatchUpdates {
    pub number: L1BatchNumber,
    pub executed_transaction_hashes: Vec<H256>,
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    pub block_execution_metrics: VmExecutionMetrics,
    pub txs_encoding_size: usize,
    pub l1_tx_count: usize,
    pub finished: Option<FinishedL1Batch>,
}

impl L1BatchUpdates {
    pub(crate) fn new(number: L1BatchNumber) -> Self {
        Self {
            number,
            executed_transaction_hashes: vec![],
            priority_ops_onchain_data: vec![],
            block_execution_metrics: VmExecutionMetrics::default(),
            txs_encoding_size: 0,
            l1_tx_count: 0,
            finished: None,
        }
    }

    pub(crate) fn extend_from_sealed_l2_block(&mut self, l2_block_updates: L2BlockUpdates) {
        let priority_ops = l2_block_updates
            .executed_transactions
            .iter()
            .filter_map(|tx| {
                if let ExecuteTransactionCommon::L1(data) = &tx.transaction.common_data {
                    Some(data.onchain_metadata().onchain_data)
                } else {
                    None
                }
            });
        self.priority_ops_onchain_data.extend(priority_ops);

        let tx_hashes = l2_block_updates
            .executed_transactions
            .iter()
            .map(|tx| tx.hash);
        self.executed_transaction_hashes.extend(tx_hashes);

        self.block_execution_metrics += l2_block_updates.block_execution_metrics;
        self.txs_encoding_size += l2_block_updates.txs_encoding_size;
        self.l1_tx_count += l2_block_updates.l1_tx_count;
    }
}

#[cfg(test)]
mod tests {
    use zksync_multivm::vm_latest::TransactionVmExt;
    use zksync_types::{L2BlockNumber, ProtocolVersionId, H256};

    use super::*;
    use crate::tests::{create_execution_result, create_transaction};

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
            create_execution_result([]),
            VmExecutionMetrics::default(),
            vec![],
        );

        let mut l1_batch_accumulator = L1BatchUpdates::new(L1BatchNumber(1));
        l1_batch_accumulator.extend_from_sealed_l2_block(l2_block_accumulator);

        assert_eq!(l1_batch_accumulator.executed_transaction_hashes.len(), 1);
        assert_eq!(l1_batch_accumulator.priority_ops_onchain_data.len(), 0);
        assert_eq!(
            l1_batch_accumulator.block_execution_metrics.l2_to_l1_logs,
            0
        );
        assert_eq!(l1_batch_accumulator.txs_encoding_size, expected_tx_size);
        assert_eq!(l1_batch_accumulator.l1_tx_count, 0);
    }
}
