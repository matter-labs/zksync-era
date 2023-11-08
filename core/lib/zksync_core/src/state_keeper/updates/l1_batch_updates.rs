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
        for tx in &miniblock_updates.executed_transactions {
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
    use zksync_types::{ProtocolVersionId, H256};

    use super::*;
    use crate::{
        gas_tracker::new_block_gas_count,
        state_keeper::tests::{create_execution_result, create_transaction},
    };
    use multivm::vm_latest::TransactionVmExt;

    #[test]
    fn apply_miniblock_with_empty_tx() {
        let mut miniblock_accumulator =
            MiniblockUpdates::new(0, 0, H256::zero(), 1, Some(ProtocolVersionId::latest()));
        let tx = create_transaction(10, 100);
        let expected_tx_size = tx.bootloader_encoding_size();

        miniblock_accumulator.extend_from_executed_transaction(
            tx,
            create_execution_result(0, []),
            BlockGasCount::default(),
            ExecutionMetrics::default(),
            vec![],
            vec![],
        );

        let mut l1_batch_accumulator = L1BatchUpdates::new();
        l1_batch_accumulator.extend_from_sealed_miniblock(miniblock_accumulator);

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
