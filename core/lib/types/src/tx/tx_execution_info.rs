use crate::fee::TransactionExecutionMetrics;
use crate::l2_to_l1_log::L2ToL1Log;
use crate::{
    commitment::SerializeCommitment,
    writes::{
        InitialStorageWrite, RepeatedStorageWrite, BYTES_PER_DERIVED_KEY,
        BYTES_PER_ENUMERATION_INDEX,
    },
    ProtocolVersionId,
};
use std::ops::{Add, AddAssign};

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
    pub total_updated_values_size: usize,
}

impl DeduplicatedWritesMetrics {
    pub fn from_tx_metrics(tx_metrics: &TransactionExecutionMetrics) -> Self {
        Self {
            initial_storage_writes: tx_metrics.initial_storage_writes,
            repeated_storage_writes: tx_metrics.repeated_storage_writes,
            total_updated_values_size: tx_metrics.total_updated_values_size,
        }
    }

    pub fn size(&self, protocol_version: ProtocolVersionId) -> usize {
        if protocol_version.is_pre_boojum() {
            self.initial_storage_writes * InitialStorageWrite::SERIALIZED_SIZE
                + self.repeated_storage_writes * RepeatedStorageWrite::SERIALIZED_SIZE
        } else {
            self.total_updated_values_size
                + (BYTES_PER_DERIVED_KEY as usize) * self.initial_storage_writes
                + (BYTES_PER_ENUMERATION_INDEX as usize) * self.repeated_storage_writes
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct ExecutionMetrics {
    pub gas_used: usize,
    pub published_bytecode_bytes: usize,
    pub l2_l1_long_messages: usize,
    pub l2_to_l1_logs: usize,
    pub contracts_used: usize,
    pub contracts_deployed: u16,
    pub vm_events: usize,
    pub storage_logs: usize,
    pub total_log_queries: usize,
    pub cycles_used: u32,
    pub computational_gas_used: u32,
    pub pubdata_published: u32,
}

impl ExecutionMetrics {
    pub fn from_tx_metrics(tx_metrics: &TransactionExecutionMetrics) -> Self {
        Self {
            published_bytecode_bytes: tx_metrics.published_bytecode_bytes,
            l2_l1_long_messages: tx_metrics.l2_l1_long_messages,
            l2_to_l1_logs: tx_metrics.l2_l1_logs,
            contracts_deployed: tx_metrics.contracts_deployed,
            contracts_used: tx_metrics.contracts_used,
            gas_used: tx_metrics.gas_used,
            storage_logs: tx_metrics.storage_logs,
            vm_events: tx_metrics.vm_events,
            total_log_queries: tx_metrics.total_log_queries,
            cycles_used: tx_metrics.cycles_used,
            computational_gas_used: tx_metrics.computational_gas_used,
            pubdata_published: tx_metrics.pubdata_published,
        }
    }

    pub fn size(&self) -> usize {
        self.l2_to_l1_logs * L2ToL1Log::SERIALIZED_SIZE
            + self.l2_l1_long_messages
            + self.published_bytecode_bytes
            // TODO(PLA-648): refactor this constant
            // It represents the need to store the length's of messages as well as bytecodes. 
            // It works due to the fact that each bytecode/L2->L1 long message is accompanied by a corresponding
            // user L2->L1 log.  
            + self.l2_to_l1_logs * 4
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
            l2_to_l1_logs: self.l2_to_l1_logs + other.l2_to_l1_logs,
            gas_used: self.gas_used + other.gas_used,
            vm_events: self.vm_events + other.vm_events,
            storage_logs: self.storage_logs + other.storage_logs,
            total_log_queries: self.total_log_queries + other.total_log_queries,
            cycles_used: self.cycles_used + other.cycles_used,
            computational_gas_used: self.computational_gas_used + other.computational_gas_used,
            pubdata_published: self.pubdata_published + other.pubdata_published,
        }
    }
}

impl AddAssign for ExecutionMetrics {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}
