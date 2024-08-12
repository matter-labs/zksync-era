use zksync_system_constants::PUBLISH_BYTECODE_OVERHEAD;
use zksync_types::{
    event::{extract_long_l2_to_l1_messages, extract_published_bytecodes},
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    tx::ExecutionMetrics,
    StorageLogWithPreviousValue, Transaction, VmEvent,
};
use zksync_utils::bytecode::bytecode_len_in_bytes;

use crate::interface::{Halt, VmExecutionStatistics, VmRevertReason};

/// Refunds produced for the user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Refunds {
    pub gas_refunded: u64,
    pub operator_suggested_refund: u64,
}

/// Events/storage logs/l2->l1 logs created within transaction execution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VmExecutionLogs {
    pub storage_logs: Vec<StorageLogWithPreviousValue>,
    pub events: Vec<VmEvent>,
    // For pre-boojum VMs, there was no distinction between user logs and system
    // logs and so all the outputted logs were treated as user_l2_to_l1_logs.
    pub user_l2_to_l1_logs: Vec<UserL2ToL1Log>,
    pub system_l2_to_l1_logs: Vec<SystemL2ToL1Log>,
    // This field moved to statistics, but we need to keep it for backward compatibility
    pub total_log_queries_count: usize,
}

impl VmExecutionLogs {
    pub fn total_l2_to_l1_logs_count(&self) -> usize {
        self.user_l2_to_l1_logs.len() + self.system_l2_to_l1_logs.len()
    }
}

/// Result and logs of the VM execution.
#[derive(Debug, Clone)]
pub struct VmExecutionResultAndLogs {
    pub result: ExecutionResult,
    pub logs: VmExecutionLogs,
    pub statistics: VmExecutionStatistics,
    pub refunds: Refunds,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionResult {
    /// Returned successfully
    Success { output: Vec<u8> },
    /// Reverted by contract
    Revert { output: VmRevertReason },
    /// Reverted for various reasons
    Halt { reason: Halt },
}

impl ExecutionResult {
    /// Returns `true` if the execution was failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Revert { .. } | Self::Halt { .. })
    }
}

impl VmExecutionResultAndLogs {
    pub fn get_execution_metrics(&self, tx: Option<&Transaction>) -> ExecutionMetrics {
        let contracts_deployed = tx
            .map(|tx| tx.execute.factory_deps.len() as u16)
            .unwrap_or(0);

        // We published the data as ABI-encoded `bytes`, so the total length is:
        // - message length in bytes, rounded up to a multiple of 32
        // - 32 bytes of encoded offset
        // - 32 bytes of encoded length
        let l2_l1_long_messages = extract_long_l2_to_l1_messages(&self.logs.events)
            .iter()
            .map(|event| (event.len() + 31) / 32 * 32 + 64)
            .sum();

        let published_bytecode_bytes = extract_published_bytecodes(&self.logs.events)
            .iter()
            .map(|bytecodehash| {
                bytecode_len_in_bytes(*bytecodehash) + PUBLISH_BYTECODE_OVERHEAD as usize
            })
            .sum();

        ExecutionMetrics {
            gas_used: self.statistics.gas_used as usize,
            published_bytecode_bytes,
            l2_l1_long_messages,
            l2_to_l1_logs: self.logs.total_l2_to_l1_logs_count(),
            contracts_used: self.statistics.contracts_used,
            contracts_deployed,
            vm_events: self.logs.events.len(),
            storage_logs: self.logs.storage_logs.len(),
            total_log_queries: self.statistics.total_log_queries,
            cycles_used: self.statistics.cycles_used,
            computational_gas_used: self.statistics.computational_gas_used,
            pubdata_published: self.statistics.pubdata_published,
            circuit_statistic: self.statistics.circuit_statistic,
        }
    }
}
