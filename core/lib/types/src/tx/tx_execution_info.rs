use crate::commitment::SerializeCommitment;
use crate::event::{extract_long_l2_to_l1_messages, extract_published_bytecodes};
use crate::fee::TransactionExecutionMetrics;
use crate::l2_to_l1_log::L2ToL1Log;
use crate::writes::{InitialStorageWrite, RepeatedStorageWrite};
use crate::{StorageLogQuery, VmEvent, PUBLISH_BYTECODE_OVERHEAD};
use std::ops::{Add, AddAssign};
use zksync_utils::bytecode::bytecode_len_in_bytes;

/// Events/storage logs/l2->l1 logs created within transaction execution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VmExecutionLogs {
    pub storage_logs: Vec<StorageLogQuery>,
    pub events: Vec<VmEvent>,
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
    pub total_log_queries_count: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TxExecutionStatus {
    Success,
    Failure,
}

impl TxExecutionStatus {
    pub fn from_has_failed(has_failed: bool) -> Self {
        if has_failed {
            Self::Failure
        } else {
            Self::Success
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct DeduplicatedWritesMetrics {
    pub initial_storage_writes: usize,
    pub repeated_storage_writes: usize,
}

impl DeduplicatedWritesMetrics {
    pub fn from_tx_metrics(tx_metrics: &TransactionExecutionMetrics) -> Self {
        Self {
            initial_storage_writes: tx_metrics.initial_storage_writes,
            repeated_storage_writes: tx_metrics.repeated_storage_writes,
        }
    }

    pub fn size(&self) -> usize {
        self.initial_storage_writes * InitialStorageWrite::SERIALIZED_SIZE
            + self.repeated_storage_writes * RepeatedStorageWrite::SERIALIZED_SIZE
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct ExecutionMetrics {
    pub gas_used: usize,
    pub published_bytecode_bytes: usize,
    pub l2_l1_long_messages: usize,
    pub l2_l1_logs: usize,
    pub contracts_used: usize,
    pub contracts_deployed: u16,
    pub vm_events: usize,
    pub storage_logs: usize,
    pub total_log_queries: usize,
    pub cycles_used: u32,
    pub computational_gas_used: u32,
}

impl ExecutionMetrics {
    pub fn from_tx_metrics(tx_metrics: &TransactionExecutionMetrics) -> Self {
        Self {
            published_bytecode_bytes: tx_metrics.published_bytecode_bytes,
            l2_l1_long_messages: tx_metrics.l2_l1_long_messages,
            l2_l1_logs: tx_metrics.l2_l1_logs,
            contracts_deployed: tx_metrics.contracts_deployed,
            contracts_used: tx_metrics.contracts_used,
            gas_used: tx_metrics.gas_used,
            storage_logs: tx_metrics.storage_logs,
            vm_events: tx_metrics.vm_events,
            total_log_queries: tx_metrics.total_log_queries,
            cycles_used: tx_metrics.cycles_used,
            computational_gas_used: tx_metrics.computational_gas_used,
        }
    }

    pub fn size(&self) -> usize {
        self.l2_l1_logs * L2ToL1Log::SERIALIZED_SIZE
            + self.l2_l1_long_messages
            + self.published_bytecode_bytes
    }

    pub fn new(
        logs: &VmExecutionLogs,
        gas_used: usize,
        contracts_deployed: u16,
        contracts_used: usize,
        cycles_used: u32,
        computational_gas_used: u32,
    ) -> Self {
        // We published the data as ABI-encoded `bytes`, so the total length is:
        // - message length in bytes, rounded up to a multiple of 32
        // - 32 bytes of encoded offset
        // - 32 bytes of encoded length
        let l2_l1_long_messages = extract_long_l2_to_l1_messages(&logs.events)
            .iter()
            .map(|event| (event.len() + 31) / 32 * 32 + 64)
            .sum();

        let published_bytecode_bytes = extract_published_bytecodes(&logs.events)
            .iter()
            .map(|bytecodehash| {
                bytecode_len_in_bytes(*bytecodehash) + PUBLISH_BYTECODE_OVERHEAD as usize
            })
            .sum();

        ExecutionMetrics {
            gas_used,
            published_bytecode_bytes,
            l2_l1_long_messages,
            l2_l1_logs: logs.l2_to_l1_logs.len(),
            contracts_used,
            contracts_deployed,
            vm_events: logs.events.len(),
            storage_logs: logs.storage_logs.len(),
            total_log_queries: logs.total_log_queries_count,
            cycles_used,
            computational_gas_used,
        }
    }
}

impl Add for ExecutionMetrics {
    type Output = ExecutionMetrics;

    fn add(self, other: ExecutionMetrics) -> ExecutionMetrics {
        ExecutionMetrics {
            published_bytecode_bytes: self.published_bytecode_bytes
                + other.published_bytecode_bytes,
            contracts_deployed: self.contracts_deployed + other.contracts_deployed,
            contracts_used: self.contracts_used + other.contracts_used,
            l2_l1_long_messages: self.l2_l1_long_messages + other.l2_l1_long_messages,
            l2_l1_logs: self.l2_l1_logs + other.l2_l1_logs,
            gas_used: self.gas_used + other.gas_used,
            vm_events: self.vm_events + other.vm_events,
            storage_logs: self.storage_logs + other.storage_logs,
            total_log_queries: self.total_log_queries + other.total_log_queries,
            cycles_used: self.cycles_used + other.cycles_used,
            computational_gas_used: self.computational_gas_used + other.computational_gas_used,
        }
    }
}

impl AddAssign for ExecutionMetrics {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}
