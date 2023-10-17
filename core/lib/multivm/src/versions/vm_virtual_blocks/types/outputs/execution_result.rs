use crate::vm_virtual_blocks::{Halt, VmExecutionStatistics, VmRevertReason};
use zksync_config::constants::PUBLISH_BYTECODE_OVERHEAD;
use zksync_types::event::{extract_long_l2_to_l1_messages, extract_published_bytecodes};
use zksync_types::tx::tx_execution_info::VmExecutionLogs;
use zksync_types::tx::ExecutionMetrics;
use zksync_types::Transaction;
use zksync_utils::bytecode::bytecode_len_in_bytes;

/// Refunds produced for the user.
#[derive(Debug, Clone, Default)]
pub struct Refunds {
    pub gas_refunded: u32,
    pub operator_suggested_refund: u32,
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
            .map(|tx| {
                tx.execute
                    .factory_deps
                    .as_ref()
                    .map_or(0, |deps| deps.len() as u16)
            })
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
            l2_l1_logs: self.logs.l2_to_l1_logs.len(),
            contracts_used: self.statistics.contracts_used,
            contracts_deployed,
            vm_events: self.logs.events.len(),
            storage_logs: self.logs.storage_logs.len(),
            total_log_queries: self.statistics.total_log_queries,
            cycles_used: self.statistics.cycles_used,
            computational_gas_used: self.statistics.computational_gas_used,
        }
    }
}
