use std::collections::HashMap;

use zksync_multivm::{
    interface::{
        Call, CompressedBytecodeInfo, ExecutionResult, L2BlockEnv, TransactionExecutionResult,
        TxExecutionStatus, VmEvent, VmExecutionMetrics, VmExecutionResultAndLogs,
    },
    vm_latest::{utils::extract_bytecodes_marked_as_known, TransactionVmExt},
};
use zksync_types::{
    block::{BlockGasCount, L2BlockHasher},
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    L2BlockNumber, ProtocolVersionId, StorageLogWithPreviousValue, Transaction, H256,
};
use zksync_utils::bytecode::hash_bytecode;

use crate::metrics::KEEPER_METRICS;

#[derive(Debug, Clone, PartialEq)]
pub struct L2BlockUpdates {
    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub events: Vec<VmEvent>,
    pub storage_logs: Vec<StorageLogWithPreviousValue>,
    pub user_l2_to_l1_logs: Vec<UserL2ToL1Log>,
    pub system_l2_to_l1_logs: Vec<SystemL2ToL1Log>,
    pub new_factory_deps: HashMap<H256, Vec<u8>>,
    /// How much L1 gas will it take to submit this block?
    pub l1_gas_count: BlockGasCount,
    pub block_execution_metrics: VmExecutionMetrics,
    pub txs_encoding_size: usize,
    pub payload_encoding_size: usize,
    pub timestamp: u64,
    pub number: L2BlockNumber,
    pub prev_block_hash: H256,
    pub virtual_blocks: u32,
    pub protocol_version: ProtocolVersionId,
}

impl L2BlockUpdates {
    pub(crate) fn new(
        timestamp: u64,
        number: L2BlockNumber,
        prev_block_hash: H256,
        virtual_blocks: u32,
        protocol_version: ProtocolVersionId,
    ) -> Self {
        Self {
            executed_transactions: vec![],
            events: vec![],
            storage_logs: vec![],
            user_l2_to_l1_logs: vec![],
            system_l2_to_l1_logs: vec![],
            new_factory_deps: HashMap::new(),
            l1_gas_count: BlockGasCount::default(),
            block_execution_metrics: VmExecutionMetrics::default(),
            txs_encoding_size: 0,
            payload_encoding_size: 0,
            timestamp,
            number,
            prev_block_hash,
            virtual_blocks,
            protocol_version,
        }
    }

    pub(crate) fn extend_from_fictive_transaction(
        &mut self,
        result: VmExecutionResultAndLogs,
        l1_gas_count: BlockGasCount,
        execution_metrics: VmExecutionMetrics,
    ) {
        self.events.extend(result.logs.events);
        self.storage_logs.extend(result.logs.storage_logs);
        self.user_l2_to_l1_logs
            .extend(result.logs.user_l2_to_l1_logs);
        self.system_l2_to_l1_logs
            .extend(result.logs.system_l2_to_l1_logs);

        self.l1_gas_count += l1_gas_count;
        self.block_execution_metrics += execution_metrics;
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn extend_from_executed_transaction(
        &mut self,
        tx: Transaction,
        tx_execution_result: VmExecutionResultAndLogs,
        tx_l1_gas_this_tx: BlockGasCount,
        execution_metrics: VmExecutionMetrics,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        new_known_factory_deps: Vec<(H256, Vec<u8>)>,
        call_traces: Vec<Call>,
    ) {
        let saved_factory_deps =
            extract_bytecodes_marked_as_known(&tx_execution_result.logs.events);
        self.new_factory_deps.extend(new_known_factory_deps.clone());
        self.events.extend(tx_execution_result.logs.events);
        self.user_l2_to_l1_logs
            .extend(tx_execution_result.logs.user_l2_to_l1_logs);
        self.system_l2_to_l1_logs
            .extend(tx_execution_result.logs.system_l2_to_l1_logs);

        let gas_refunded = tx_execution_result.refunds.gas_refunded;
        let operator_suggested_refund = tx_execution_result.refunds.operator_suggested_refund;
        let execution_status = if tx_execution_result.result.is_failed() {
            TxExecutionStatus::Failure
        } else {
            TxExecutionStatus::Success
        };

        let revert_reason = match &tx_execution_result.result {
            ExecutionResult::Success { .. } => {
                KEEPER_METRICS.inc_succeeded_txs();
                None
            }
            ExecutionResult::Revert { output } => {
                KEEPER_METRICS.inc_reverted_txs(output);
                Some(output.to_string())
            }
            ExecutionResult::Halt { .. } => {
                unreachable!("Tx that is added to `UpdatesManager` must not have Halted status")
            }
        };

        // Get transaction factory deps
        let factory_deps = &tx.execute.factory_deps;
        let mut tx_factory_deps: HashMap<_, _> = factory_deps
            .iter()
            .map(|bytecode| (hash_bytecode(bytecode), bytecode.clone()))
            .collect();
        new_known_factory_deps
            .into_iter()
            .for_each(|(hash, bytecode)| {
                tx_factory_deps.insert(hash, bytecode);
            });

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

        self.l1_gas_count += tx_l1_gas_this_tx;
        self.block_execution_metrics += execution_metrics;
        self.txs_encoding_size += tx.bootloader_encoding_size();
        self.payload_encoding_size +=
            zksync_protobuf::repr::encode::<zksync_dal::consensus::proto::Transaction>(&tx).len();
        self.storage_logs
            .extend(tx_execution_result.logs.storage_logs);

        self.executed_transactions.push(TransactionExecutionResult {
            hash: tx.hash(),
            transaction: tx,
            execution_info: execution_metrics,
            execution_status,
            refunded_gas: gas_refunded,
            operator_suggested_refund,
            compressed_bytecodes,
            call_traces,
            revert_reason,
        });
    }

    /// Calculates L2 block hash based on the protocol version.
    pub(crate) fn get_l2_block_hash(&self) -> H256 {
        let mut digest = L2BlockHasher::new(self.number, self.timestamp, self.prev_block_hash);
        for tx in &self.executed_transactions {
            digest.push_tx_hash(tx.hash);
        }
        digest.finalize(self.protocol_version)
    }

    pub(crate) fn get_env(&self) -> L2BlockEnv {
        L2BlockEnv {
            number: self.number.0,
            timestamp: self.timestamp,
            prev_block_hash: self.prev_block_hash,
            max_virtual_blocks_to_create: self.virtual_blocks,
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_multivm::vm_latest::TransactionVmExt;

    use super::*;
    use crate::tests::{create_execution_result, create_transaction};

    #[test]
    fn apply_empty_l2_tx() {
        let mut accumulator = L2BlockUpdates::new(
            0,
            L2BlockNumber(0),
            H256::random(),
            0,
            ProtocolVersionId::latest(),
        );
        let tx = create_transaction(10, 100);
        let bootloader_encoding_size = tx.bootloader_encoding_size();
        let payload_encoding_size =
            zksync_protobuf::repr::encode::<zksync_dal::consensus::proto::Transaction>(&tx).len();

        accumulator.extend_from_executed_transaction(
            tx,
            create_execution_result([]),
            BlockGasCount::default(),
            VmExecutionMetrics::default(),
            vec![],
            vec![],
            vec![],
        );

        assert_eq!(accumulator.executed_transactions.len(), 1);
        assert_eq!(accumulator.events.len(), 0);
        assert_eq!(accumulator.storage_logs.len(), 0);
        assert_eq!(accumulator.user_l2_to_l1_logs.len(), 0);
        assert_eq!(accumulator.system_l2_to_l1_logs.len(), 0);
        assert_eq!(accumulator.l1_gas_count, Default::default());
        assert_eq!(accumulator.new_factory_deps.len(), 0);
        assert_eq!(accumulator.block_execution_metrics.l2_to_l1_logs, 0);
        assert_eq!(accumulator.txs_encoding_size, bootloader_encoding_size);
        assert_eq!(accumulator.payload_encoding_size, payload_encoding_size);
    }
}
