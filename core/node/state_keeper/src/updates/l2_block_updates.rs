use std::collections::HashMap;

use zksync_multivm::{
    interface::{
        Call, ExecutionResult, L2BlockEnv, TransactionExecutionResult, TxExecutionStatus, VmEvent,
        VmExecutionMetrics, VmExecutionResultAndLogs,
    },
    vm_latest::TransactionVmExt,
};
use zksync_types::{
    block::L2BlockHasher,
    bytecode::BytecodeHash,
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    L2BlockNumber, ProtocolVersionId, StorageLogWithPreviousValue, Transaction, H256,
};

use crate::metrics::KEEPER_METRICS;

#[derive(Debug, Clone, PartialEq)]
pub struct L2BlockUpdates {
    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub events: Vec<VmEvent>,
    pub storage_logs: Vec<StorageLogWithPreviousValue>,
    pub user_l2_to_l1_logs: Vec<UserL2ToL1Log>,
    pub system_l2_to_l1_logs: Vec<SystemL2ToL1Log>,
    pub new_factory_deps: HashMap<H256, Vec<u8>>,
    pub block_execution_metrics: VmExecutionMetrics,
    pub txs_encoding_size: usize,
    pub payload_encoding_size: usize,
    pub l1_tx_count: usize,
    pub number: L2BlockNumber,
    pub prev_block_hash: H256,
    pub prev_block_timestamp: Option<u64>,
    pub virtual_blocks: u32,
    pub protocol_version: ProtocolVersionId,
    timestamp_ms: u64,
}

impl L2BlockUpdates {
    pub(crate) fn new(
        timestamp_ms: u64,
        number: L2BlockNumber,
        prev_block_hash: H256,
        prev_block_timestamp: Option<u64>,
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
            block_execution_metrics: VmExecutionMetrics::default(),
            txs_encoding_size: 0,
            payload_encoding_size: 0,
            l1_tx_count: 0,
            timestamp_ms,
            number,
            prev_block_hash,
            prev_block_timestamp,
            virtual_blocks,
            protocol_version,
        }
    }

    pub(crate) fn extend_from_fictive_transaction(
        &mut self,
        result: VmExecutionResultAndLogs,
        execution_metrics: VmExecutionMetrics,
    ) {
        self.events.extend(result.logs.events);
        self.storage_logs.extend(result.logs.storage_logs);
        self.user_l2_to_l1_logs
            .extend(result.logs.user_l2_to_l1_logs);
        self.system_l2_to_l1_logs
            .extend(result.logs.system_l2_to_l1_logs);

        self.block_execution_metrics += execution_metrics;
    }

    pub(crate) fn extend_from_executed_transaction(
        &mut self,
        tx: Transaction,
        tx_execution_result: VmExecutionResultAndLogs,
        execution_metrics: VmExecutionMetrics,
        call_traces: Vec<Call>,
    ) {
        let saved_factory_deps =
            VmEvent::extract_bytecodes_marked_as_known(&tx_execution_result.logs.events);

        let gas_refunded = tx_execution_result.refunds.gas_refunded;
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
            .map(|bytecode| {
                (
                    BytecodeHash::for_bytecode(bytecode).value(),
                    bytecode.clone(),
                )
            })
            .collect();
        // Ensure that *dynamic* factory deps (ones that may be created when executing EVM contracts)
        // are added into the lookup map as well.
        tx_factory_deps.extend(tx_execution_result.dynamic_factory_deps);

        // Save all bytecodes that were marked as known in the bootloader
        let known_bytecodes = saved_factory_deps.map(|bytecode_hash| {
            let bytecode = tx_factory_deps.get(&bytecode_hash).unwrap_or_else(|| {
                panic!(
                    "Failed to get factory deps on tx: bytecode hash: {:?}, tx hash: {}",
                    bytecode_hash,
                    tx.hash()
                )
            });
            (bytecode_hash, bytecode.clone())
        });
        self.new_factory_deps.extend(known_bytecodes);

        self.block_execution_metrics += execution_metrics;
        self.txs_encoding_size += tx.bootloader_encoding_size();
        self.payload_encoding_size +=
            zksync_protobuf::repr::encode::<zksync_dal::consensus::proto::Transaction>(&tx).len();
        self.events.extend(tx_execution_result.logs.events);
        self.user_l2_to_l1_logs
            .extend(tx_execution_result.logs.user_l2_to_l1_logs);
        self.system_l2_to_l1_logs
            .extend(tx_execution_result.logs.system_l2_to_l1_logs);
        self.storage_logs
            .extend(tx_execution_result.logs.storage_logs);
        if tx.is_l1() {
            self.l1_tx_count += 1;
        }

        self.executed_transactions.push(TransactionExecutionResult {
            hash: tx.hash(),
            transaction: tx,
            execution_info: execution_metrics,
            execution_status,
            refunded_gas: gas_refunded,
            call_traces,
            revert_reason,
        });
    }

    /// Calculates L2 block hash based on the protocol version.
    pub(crate) fn get_l2_block_hash(&self) -> H256 {
        let mut digest = L2BlockHasher::new(self.number, self.timestamp(), self.prev_block_hash);
        for tx in &self.executed_transactions {
            digest.push_tx_hash(tx.hash);
        }
        digest.finalize(self.protocol_version)
    }

    pub(crate) fn get_env(&self) -> L2BlockEnv {
        L2BlockEnv {
            number: self.number.0,
            timestamp: self.timestamp(),
            prev_block_hash: self.prev_block_hash,
            max_virtual_blocks_to_create: self.virtual_blocks,
        }
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp_ms / 1000
    }

    pub fn timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    #[cfg(test)]
    pub fn new_with_data(
        number: L2BlockNumber,
        timestamp_ms: u64,
        executed_transactions: Vec<TransactionExecutionResult>,
        events: Vec<VmEvent>,
        storage_logs: Vec<StorageLogWithPreviousValue>,
        user_l2_to_l1_logs: Vec<UserL2ToL1Log>,
        new_factory_deps: HashMap<H256, Vec<u8>>,
    ) -> Self {
        Self {
            executed_transactions,
            events,
            storage_logs,
            user_l2_to_l1_logs,
            system_l2_to_l1_logs: Default::default(),
            new_factory_deps,
            block_execution_metrics: Default::default(),
            txs_encoding_size: Default::default(),
            payload_encoding_size: Default::default(),
            l1_tx_count: 0,
            timestamp_ms,
            number,
            prev_block_hash: Default::default(),
            virtual_blocks: Default::default(),
            protocol_version: ProtocolVersionId::latest(),
            prev_block_timestamp: None,
        }
    }

    #[cfg(test)]
    pub fn set_timestamp_ms(&mut self, value: u64) {
        self.timestamp_ms = value;
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
            None,
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
            VmExecutionMetrics::default(),
            vec![],
        );

        assert_eq!(accumulator.executed_transactions.len(), 1);
        assert_eq!(accumulator.events.len(), 0);
        assert_eq!(accumulator.storage_logs.len(), 0);
        assert_eq!(accumulator.user_l2_to_l1_logs.len(), 0);
        assert_eq!(accumulator.system_l2_to_l1_logs.len(), 0);
        assert_eq!(accumulator.new_factory_deps.len(), 0);
        assert_eq!(accumulator.block_execution_metrics.l2_to_l1_logs, 0);
        assert_eq!(accumulator.txs_encoding_size, bootloader_encoding_size);
        assert_eq!(accumulator.payload_encoding_size, payload_encoding_size);
        assert_eq!(accumulator.l1_tx_count, 0);
    }
}
