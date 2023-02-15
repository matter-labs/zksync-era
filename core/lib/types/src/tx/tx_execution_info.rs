use crate::commitment::CommitmentSerializable;
use crate::event::{extract_long_l2_to_l1_messages, extract_published_bytecodes};
use crate::l2_to_l1_log::L2ToL1Log;
use crate::log_query_sorter::sort_storage_access_queries;
use crate::writes::{InitialStorageWrite, RepeatedStorageWrite};
use crate::{StorageLogQuery, StorageLogQueryType, VmEvent};
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

#[derive(Debug, Clone, Copy, Default, serde::Serialize, PartialEq)]
pub struct ExecutionMetrics {
    pub initial_storage_writes: usize,
    pub repeated_storage_writes: usize,
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
}

impl ExecutionMetrics {
    pub fn storage_writes(&self) -> usize {
        self.initial_storage_writes + self.repeated_storage_writes
    }

    pub fn size(&self) -> usize {
        self.initial_storage_writes * InitialStorageWrite::SERIALIZED_SIZE
            + self.repeated_storage_writes * RepeatedStorageWrite::SERIALIZED_SIZE
            + self.l2_l1_logs * L2ToL1Log::SERIALIZED_SIZE
            + self.l2_l1_long_messages
            + self.published_bytecode_bytes
    }

    pub fn new(
        logs: &VmExecutionLogs,
        gas_used: usize,
        contracts_deployed: u16,
        contracts_used: usize,
        cycles_used: u32,
    ) -> Self {
        let (initial_storage_writes, repeated_storage_writes) =
            get_initial_and_repeated_storage_writes(logs.storage_logs.as_slice());

        let l2_l1_long_messages = extract_long_l2_to_l1_messages(&logs.events)
            .iter()
            .map(|event| event.len())
            .sum();

        let published_bytecode_bytes = extract_published_bytecodes(&logs.events)
            .iter()
            .map(|bytecodehash| bytecode_len_in_bytes(*bytecodehash))
            .sum();

        ExecutionMetrics {
            initial_storage_writes: initial_storage_writes as usize,
            repeated_storage_writes: repeated_storage_writes as usize,
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
        }
    }
}

impl Add for ExecutionMetrics {
    type Output = ExecutionMetrics;

    fn add(self, other: ExecutionMetrics) -> ExecutionMetrics {
        ExecutionMetrics {
            initial_storage_writes: self.initial_storage_writes + other.initial_storage_writes,
            repeated_storage_writes: self.repeated_storage_writes + other.repeated_storage_writes,
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
        }
    }
}

impl AddAssign for ExecutionMetrics {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

pub fn get_initial_and_repeated_storage_writes(
    storage_log_queries: &[StorageLogQuery],
) -> (u32, u32) {
    let mut initial_storage_writes = 0;
    let mut repeated_storage_writes = 0;

    let (_, deduped_storage_logs) = sort_storage_access_queries(storage_log_queries);
    for log in &deduped_storage_logs {
        match log.log_type {
            StorageLogQueryType::InitialWrite => {
                initial_storage_writes += 1;
            }
            StorageLogQueryType::RepeatedWrite => repeated_storage_writes += 1,
            StorageLogQueryType::Read => {}
        }
    }
    (initial_storage_writes, repeated_storage_writes)
}
