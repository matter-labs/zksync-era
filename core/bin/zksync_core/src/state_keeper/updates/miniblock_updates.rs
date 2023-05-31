use std::collections::HashMap;
use vm::transaction_data::TransactionData;
use vm::vm::VmTxExecutionResult;
use zksync_types::block::BlockGasCount;
use zksync_types::event::extract_bytecodes_marked_as_known;
use zksync_types::l2_to_l1_log::L2ToL1Log;
use zksync_types::tx::tx_execution_info::VmExecutionLogs;
use zksync_types::tx::ExecutionMetrics;
use zksync_types::{tx::TransactionExecutionResult, StorageLogQuery, Transaction, VmEvent, H256};
use zksync_utils::bytecode::{hash_bytecode, CompressedBytecodeInfo};

#[derive(Debug, Clone, PartialEq)]
pub struct MiniblockUpdates {
    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub events: Vec<VmEvent>,
    pub storage_logs: Vec<StorageLogQuery>,
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
    pub new_factory_deps: HashMap<H256, Vec<u8>>,
    // how much L1 gas will it take to submit this block?
    pub l1_gas_count: BlockGasCount,
    pub block_execution_metrics: ExecutionMetrics,
    pub txs_encoding_size: usize,

    pub timestamp: u64,
}

impl MiniblockUpdates {
    pub(crate) fn new(timestamp: u64) -> Self {
        Self {
            executed_transactions: Default::default(),
            events: Default::default(),
            storage_logs: Default::default(),
            l2_to_l1_logs: Default::default(),
            new_factory_deps: Default::default(),
            l1_gas_count: Default::default(),
            block_execution_metrics: Default::default(),
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
        tx: &Transaction,
        tx_execution_result: VmTxExecutionResult,
        tx_l1_gas_this_tx: BlockGasCount,
        execution_metrics: ExecutionMetrics,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    ) {
        // Get bytecode hashes that were marked as known
        let saved_factory_deps =
            extract_bytecodes_marked_as_known(&tx_execution_result.result.logs.events);

        // Get transaction factory deps
        let tx_factory_deps: HashMap<_, _> = tx
            .execute
            .factory_deps
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|bytecode| (hash_bytecode(bytecode), bytecode.clone()))
            .collect();

        // Save all bytecodes that were marked as known on the bootloader
        saved_factory_deps.into_iter().for_each(|bytecodehash| {
            let bytecode = tx_factory_deps
                .get(&bytecodehash)
                .unwrap_or_else(|| {
                    panic!(
                        "Failed to get factory deps on tx: bytecode hash: {:?}, tx hash: {}",
                        bytecodehash,
                        tx.hash()
                    )
                })
                .clone();

            self.new_factory_deps.insert(bytecodehash, bytecode);
        });

        self.executed_transactions.push(TransactionExecutionResult {
            transaction: tx.clone(),
            hash: tx.hash(),
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

        self.events.extend(tx_execution_result.result.logs.events);
        self.storage_logs
            .extend(tx_execution_result.result.logs.storage_logs);
        self.l2_to_l1_logs
            .extend(tx_execution_result.result.logs.l2_to_l1_logs);

        self.l1_gas_count += tx_l1_gas_this_tx;
        self.block_execution_metrics += execution_metrics;

        let tx_data: TransactionData = tx.clone().into();
        self.txs_encoding_size += tx_data.into_tokens().len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm::vm::{VmPartialExecutionResult, VmTxExecutionResult};
    use zksync_types::{l2::L2Tx, tx::tx_execution_info::TxExecutionStatus, Address, Nonce, U256};

    #[test]
    fn apply_empty_l2_tx() {
        let mut accumulator = MiniblockUpdates::new(0);

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

        accumulator.extend_from_executed_transaction(
            &tx,
            VmTxExecutionResult {
                status: TxExecutionStatus::Success,
                result: VmPartialExecutionResult {
                    logs: Default::default(),
                    revert_reason: None,
                    contracts_used: 0,
                    cycles_used: 0,
                    computational_gas_used: 0,
                },
                call_traces: vec![],
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
            Default::default(),
            Default::default(),
            Default::default(),
        );

        assert_eq!(accumulator.executed_transactions.len(), 1);
        assert_eq!(accumulator.events.len(), 0);
        assert_eq!(accumulator.storage_logs.len(), 0);
        assert_eq!(accumulator.l2_to_l1_logs.len(), 0);
        assert_eq!(accumulator.l1_gas_count, Default::default());
        assert_eq!(accumulator.new_factory_deps.len(), 0);
        assert_eq!(accumulator.block_execution_metrics.l2_l1_logs, 0);

        let tx_data: TransactionData = tx.into();
        assert_eq!(accumulator.txs_encoding_size, tx_data.into_tokens().len());
    }
}
