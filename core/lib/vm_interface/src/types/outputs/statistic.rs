use std::ops;

use serde::{Deserialize, Serialize};
use zksync_types::{
    commitment::SerializeCommitment,
    l2_to_l1_log::L2ToL1Log,
    writes::{
        InitialStorageWrite, RepeatedStorageWrite, BYTES_PER_DERIVED_KEY,
        BYTES_PER_ENUMERATION_INDEX,
    },
    ProtocolVersionId,
};

/// Holds information about number of circuits used per circuit type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CircuitStatistic {
    pub main_vm: f32,
    pub ram_permutation: f32,
    pub storage_application: f32,
    pub storage_sorter: f32,
    pub code_decommitter: f32,
    pub code_decommitter_sorter: f32,
    pub log_demuxer: f32,
    pub events_sorter: f32,
    pub keccak256: f32,
    pub ecrecover: f32,
    pub sha256: f32,
    #[serde(default)]
    pub secp256k1_verify: f32,
    #[serde(default)]
    pub transient_storage_checker: f32,
    #[serde(default)]
    pub modexp: f32,
    #[serde(default)]
    pub ecadd: f32,
    #[serde(default)]
    pub ecmul: f32,
    #[serde(default)]
    pub ecpairing: f32,
}

impl CircuitStatistic {
    /// Rounds up numbers and adds them.
    pub fn total(&self) -> usize {
        self.main_vm.ceil() as usize
            + self.ram_permutation.ceil() as usize
            + self.storage_application.ceil() as usize
            + self.storage_sorter.ceil() as usize
            + self.code_decommitter.ceil() as usize
            + self.code_decommitter_sorter.ceil() as usize
            + self.log_demuxer.ceil() as usize
            + self.events_sorter.ceil() as usize
            + self.keccak256.ceil() as usize
            + self.ecrecover.ceil() as usize
            + self.sha256.ceil() as usize
            + self.secp256k1_verify.ceil() as usize
            + self.transient_storage_checker.ceil() as usize
            + self.modexp as usize
            + self.ecadd as usize
            + self.ecmul as usize
            + self.ecpairing as usize
    }

    /// Adds numbers.
    pub fn total_f32(&self) -> f32 {
        self.main_vm
            + self.ram_permutation
            + self.storage_application
            + self.storage_sorter
            + self.code_decommitter
            + self.code_decommitter_sorter
            + self.log_demuxer
            + self.events_sorter
            + self.keccak256
            + self.ecrecover
            + self.sha256
            + self.secp256k1_verify
            + self.transient_storage_checker
            + self.modexp
            + self.ecadd
            + self.ecmul
            + self.ecpairing
    }
}

impl ops::Add for CircuitStatistic {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            main_vm: self.main_vm + other.main_vm,
            ram_permutation: self.ram_permutation + other.ram_permutation,
            storage_application: self.storage_application + other.storage_application,
            storage_sorter: self.storage_sorter + other.storage_sorter,
            code_decommitter: self.code_decommitter + other.code_decommitter,
            code_decommitter_sorter: self.code_decommitter_sorter + other.code_decommitter_sorter,
            log_demuxer: self.log_demuxer + other.log_demuxer,
            events_sorter: self.events_sorter + other.events_sorter,
            keccak256: self.keccak256 + other.keccak256,
            ecrecover: self.ecrecover + other.ecrecover,
            sha256: self.sha256 + other.sha256,
            secp256k1_verify: self.secp256k1_verify + other.secp256k1_verify,
            transient_storage_checker: self.transient_storage_checker
                + other.transient_storage_checker,
            modexp: self.modexp + other.modexp,
            ecadd: self.ecadd + other.ecadd,
            ecmul: self.ecmul + other.ecmul,
            ecpairing: self.ecpairing + other.ecpairing,
        }
    }
}

/// Statistics of the tx execution.
#[derive(Debug, Default, Clone)]
pub struct VmExecutionStatistics {
    /// Number of contracts used by the VM during the tx execution.
    pub contracts_used: usize,
    /// Cycles used by the VM during the tx execution.
    pub cycles_used: u32,
    /// Gas used by the VM during the tx execution.
    pub gas_used: u64,
    /// Gas remaining after the tx execution.
    pub gas_remaining: u32,
    /// Computational gas used by the VM during the tx execution.
    pub computational_gas_used: u32,
    /// Number of log queries produced by the VM during the tx execution.
    pub total_log_queries: usize,
    pub pubdata_published: u32,
    pub circuit_statistic: CircuitStatistic,
}

/// Oracle metrics reported by legacy VMs.
#[derive(Debug, Default)]
pub struct VmMemoryMetrics {
    pub event_sink_inner: usize,
    pub event_sink_history: usize,
    pub memory_inner: usize,
    pub memory_history: usize,
    pub decommittment_processor_inner: usize,
    pub decommittment_processor_history: usize,
    pub storage_inner: usize,
    pub storage_history: usize,
}

impl VmMemoryMetrics {
    pub fn full_size(&self) -> usize {
        [
            self.event_sink_inner,
            self.event_sink_history,
            self.memory_inner,
            self.memory_history,
            self.decommittment_processor_inner,
            self.decommittment_processor_history,
            self.storage_inner,
            self.storage_history,
        ]
        .iter()
        .sum::<usize>()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct DeduplicatedWritesMetrics {
    pub initial_storage_writes: usize,
    pub repeated_storage_writes: usize,
    pub total_updated_values_size: usize,
}

impl DeduplicatedWritesMetrics {
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

/// [`VmExecutionMetrics`] + some other metrics specific for executing a single transaction (e.g.,
/// gas remaining after execution and gas refunded by the bootloader).
#[derive(Debug, Clone, Copy)]
pub struct TransactionExecutionMetrics {
    pub writes: DeduplicatedWritesMetrics,
    pub vm: VmExecutionMetrics,
    pub gas_remaining: u32,
    pub gas_refunded: u64,
}

impl Default for TransactionExecutionMetrics {
    fn default() -> Self {
        Self {
            writes: DeduplicatedWritesMetrics::default(),
            vm: VmExecutionMetrics::default(),
            gas_remaining: u32::MAX,
            gas_refunded: 0,
        }
    }
}

/// Metrics for a (part of) VM execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct VmExecutionMetrics {
    pub gas_used: usize,
    pub published_bytecode_bytes: usize,
    pub l2_l1_long_messages: usize,
    pub l2_to_l1_logs: usize,
    pub user_l2_to_l1_logs: usize,
    pub contracts_used: usize,
    pub vm_events: usize,
    pub storage_logs: usize,
    /// Sum of storage logs, vm events, l2->l1 logs, and the number of precompile calls.
    pub total_log_queries: usize,
    pub cycles_used: u32,
    pub computational_gas_used: u32,
    pub pubdata_published: u32,
    pub circuit_statistic: CircuitStatistic,
}

impl VmExecutionMetrics {
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

impl ops::Add for VmExecutionMetrics {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            published_bytecode_bytes: self.published_bytecode_bytes
                + other.published_bytecode_bytes,
            contracts_used: self.contracts_used + other.contracts_used,
            l2_l1_long_messages: self.l2_l1_long_messages + other.l2_l1_long_messages,
            l2_to_l1_logs: self.l2_to_l1_logs + other.l2_to_l1_logs,
            user_l2_to_l1_logs: self.user_l2_to_l1_logs + other.user_l2_to_l1_logs,
            gas_used: self.gas_used + other.gas_used,
            vm_events: self.vm_events + other.vm_events,
            storage_logs: self.storage_logs + other.storage_logs,
            total_log_queries: self.total_log_queries + other.total_log_queries,
            cycles_used: self.cycles_used + other.cycles_used,
            computational_gas_used: self.computational_gas_used + other.computational_gas_used,
            pubdata_published: self.pubdata_published + other.pubdata_published,
            circuit_statistic: self.circuit_statistic + other.circuit_statistic,
        }
    }
}

impl ops::AddAssign for VmExecutionMetrics {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}
