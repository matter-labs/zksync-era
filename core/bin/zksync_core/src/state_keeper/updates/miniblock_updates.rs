use std::collections::HashMap;

use vm::vm::VmTxExecutionResult;
use zksync_types::{
    block::BlockGasCount,
    event::extract_bytecodes_marked_as_known,
    l2_to_l1_log::L2ToL1Log,
    tx::{tx_execution_info::VmExecutionLogs, ExecutionMetrics, TransactionExecutionResult},
    StorageLogQuery, Transaction, VmEvent, H256,
};
use zksync_utils::bytecode::{hash_bytecode, CompressedBytecodeInfo};

use crate::state_keeper::extractors;

#[derive(Debug, Clone, PartialEq)]
pub struct MiniblockUpdates {
    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub events: Vec<VmEvent>,
    pub storage_logs: Vec<StorageLogQuery>,
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
    pub new_factory_deps: HashMap<H256, Vec<u8>>,
    /// How much L1 gas will it take to submit this block?
    pub l1_gas_count: BlockGasCount,
    pub block_execution_metrics: ExecutionMetrics,
    pub txs_encoding_size: usize,
    pub timestamp: u64,
}

impl MiniblockUpdates {
    pub(crate) fn new(timestamp: u64) -> Self {
        Self {
            executed_transactions: vec![],
            events: vec![],
            storage_logs: vec![],
            l2_to_l1_logs: vec![],
            new_factory_deps: HashMap::new(),
            l1_gas_count: BlockGasCount::default(),
            block_execution_metrics: ExecutionMetrics::default(),
            txs_encoding_size: 0,
            timestamp,
        }
    }

    pub(crate) fn extend_from_fictive_transaction(&mut self, vm_execution_logs: VmExecutionLogs) {
        self.events.extend(vm_execution_logs.events);
        self.storage_logs.extend(vm_execution_logs.storage_logs);
        self.l2_to_l1_logs.extend(vm_execution_logs.l2_to_l1_logs);
    }

    pub(crate) fn extend_from_executed_transaction(
        &mut self,
        tx: Transaction,
        tx_execution_result: VmTxExecutionResult,
        tx_l1_gas_this_tx: BlockGasCount,
        execution_metrics: ExecutionMetrics,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    ) {
        // Get bytecode hashes that were marked as known
        let saved_factory_deps =
            extract_bytecodes_marked_as_known(&tx_execution_result.result.logs.events);

        // Get transaction factory deps
        let factory_deps = tx.execute.factory_deps.as_deref().unwrap_or_default();
        let tx_factory_deps: HashMap<_, _> = factory_deps
            .iter()
            .map(|bytecode| (hash_bytecode(bytecode), bytecode))
            .collect();

        // Save all bytecodes that were marked as known on the bootloader
        let known_bytecodes = saved_factory_deps.into_iter().map(|bytecode_hash| {
            let bytecode = tx_factory_deps.get(&bytecode_hash).unwrap_or_else(|| {
                panic!(
                    "Failed to get factory deps on tx: bytecode hash: {:?}, tx hash: {}",
                    bytecode_hash,
                    tx.hash()
                )
            });
            (bytecode_hash, bytecode.to_vec())
        });
        self.new_factory_deps.extend(known_bytecodes);

        self.events.extend(tx_execution_result.result.logs.events);
        self.storage_logs
            .extend(tx_execution_result.result.logs.storage_logs);
        self.l2_to_l1_logs
            .extend(tx_execution_result.result.logs.l2_to_l1_logs);

        self.l1_gas_count += tx_l1_gas_this_tx;
        self.block_execution_metrics += execution_metrics;
        self.txs_encoding_size += extractors::encoded_transaction_size(tx.clone());

        self.executed_transactions.push(TransactionExecutionResult {
            hash: tx.hash(),
            transaction: tx,
            execution_info: execution_metrics,
            execution_status: tx_execution_result.status,
            refunded_gas: tx_execution_result.gas_refunded,
            operator_suggested_refund: tx_execution_result.operator_suggested_refund,
            compressed_bytecodes,
            call_traces: tx_execution_result.call_traces,
            revert_reason: tx_execution_result
                .result
                .revert_reason
                .map(|reason| reason.to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_keeper::tests::{create_execution_result, create_transaction};

    #[test]
    fn apply_empty_l2_tx() {
        let mut accumulator = MiniblockUpdates::new(0);
        let tx = create_transaction(10, 100);
        let expected_tx_size = extractors::encoded_transaction_size(tx.clone());

        accumulator.extend_from_executed_transaction(
            tx,
            create_execution_result(0, []),
            BlockGasCount::default(),
            ExecutionMetrics::default(),
            vec![],
        );

        assert_eq!(accumulator.executed_transactions.len(), 1);
        assert_eq!(accumulator.events.len(), 0);
        assert_eq!(accumulator.storage_logs.len(), 0);
        assert_eq!(accumulator.l2_to_l1_logs.len(), 0);
        assert_eq!(accumulator.l1_gas_count, Default::default());
        assert_eq!(accumulator.new_factory_deps.len(), 0);
        assert_eq!(accumulator.block_execution_metrics.l2_l1_logs, 0);
        assert_eq!(accumulator.txs_encoding_size, expected_tx_size);
    }
}
