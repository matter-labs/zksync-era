use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};
use zk_evm_1_4_0::aux_structures::Timestamp;

use crate::{
    interface::storage::WriteStorage,
    vm_boojum_integration::{
        old_vm::oracles::OracleWithHistory, types::internals::VmSnapshot, vm::Vm,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "stage", rename_all = "snake_case")]
enum RollbackStage {
    DecommitmentProcessorRollback,
    EventSinkRollback,
    StorageRollback,
    MemoryRollback,
    PrecompilesProcessorRollback,
    ApplyBootloaderSnapshot,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_vm_boojum_integration")]
struct VmMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    rollback_time: Family<RollbackStage, Histogram<Duration>>,
}

#[vise::register]
static METRICS: vise::Global<VmMetrics> = vise::Global::new();

/// Implementation of VM related to rollbacks inside virtual machine
impl<S: WriteStorage> Vm<S, crate::vm_latest::HistoryEnabled> {
    pub(crate) fn make_snapshot_inner(&mut self) {
        self.snapshots.push_back(VmSnapshot {
            // Vm local state contains O(1) various parameters (registers/etc).
            // The only "expensive" copying here is copying of the call stack.
            // It will take `O(callstack_depth)` to copy it.
            // So it is generally recommended to get snapshots of the bootloader frame,
            // where the depth is 1.
            local_state: self.state.local_state.clone(),
            bootloader_state: self.bootloader_state.get_snapshot(),
        });
    }

    pub(crate) fn rollback_to_snapshot(&mut self, snapshot: VmSnapshot) {
        let VmSnapshot {
            local_state,
            bootloader_state,
        } = snapshot;

        let stage_latency =
            METRICS.rollback_time[&RollbackStage::DecommitmentProcessorRollback].start();
        let timestamp = Timestamp(local_state.timestamp);
        tracing::trace!("Rolling back decomitter");
        self.state
            .decommittment_processor
            .rollback_to_timestamp(timestamp);
        stage_latency.observe();

        let stage_latency = METRICS.rollback_time[&RollbackStage::EventSinkRollback].start();
        tracing::trace!("Rolling back event_sink");
        self.state.event_sink.rollback_to_timestamp(timestamp);
        stage_latency.observe();

        let stage_latency = METRICS.rollback_time[&RollbackStage::StorageRollback].start();
        tracing::trace!("Rolling back storage");
        self.state.storage.rollback_to_timestamp(timestamp);
        stage_latency.observe();

        let stage_latency = METRICS.rollback_time[&RollbackStage::MemoryRollback].start();
        tracing::trace!("Rolling back memory");
        self.state.memory.rollback_to_timestamp(timestamp);
        stage_latency.observe();

        let stage_latency =
            METRICS.rollback_time[&RollbackStage::PrecompilesProcessorRollback].start();
        tracing::trace!("Rolling back precompiles_processor");
        self.state
            .precompiles_processor
            .rollback_to_timestamp(timestamp);
        stage_latency.observe();

        self.state.local_state = local_state;
        let stage_latency = METRICS.rollback_time[&RollbackStage::ApplyBootloaderSnapshot].start();
        self.bootloader_state.apply_snapshot(bootloader_state);
        stage_latency.observe();
    }
}
