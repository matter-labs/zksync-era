use std::time::Instant;
use zk_evm::aux_structures::Timestamp;
use zksync_state::WriteStorage;

use crate::old_vm::history_recorder::HistoryEnabled;
use crate::old_vm::oracles::OracleWithHistory;
use crate::types::internals::VmSnapshot;
use crate::vm::Vm;

/// Implementation of VM related to rollbacks inside virtual machine
impl<S: WriteStorage> Vm<S, HistoryEnabled> {
    pub(crate) fn make_snapshot_inner(&mut self) {
        self.snapshots.push(VmSnapshot {
            // Vm local state contains O(1) various parameters (registers/etc).
            // The only "expensive" copying here is copying of the callstack.
            // It will take O(callstack_depth) to copy it.
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

        let mut stage_started_at = Instant::now();
        let timestamp = Timestamp(local_state.timestamp);
        vlog::trace!("Rolling back decomitter");
        self.state
            .decommittment_processor
            .rollback_to_timestamp(timestamp);
        metrics::histogram!(
            "server.vm.rollback_time",
            stage_started_at.elapsed(),
            "stage" => "decommittment_processor_rollback"
        );

        stage_started_at = Instant::now();
        vlog::trace!("Rolling back event_sink");
        self.state.event_sink.rollback_to_timestamp(timestamp);
        metrics::histogram!(
            "server.vm.rollback_time",
            stage_started_at.elapsed(),
            "stage" => "event_sink_rollback"
        );

        stage_started_at = Instant::now();
        vlog::trace!("Rolling back storage");
        self.state.storage.rollback_to_timestamp(timestamp);
        metrics::histogram!(
            "server.vm.rollback_time",
            stage_started_at.elapsed(),
            "stage" => "event_sink_rollback"
        );

        stage_started_at = Instant::now();
        vlog::trace!("Rolling back memory");
        self.state.memory.rollback_to_timestamp(timestamp);
        metrics::histogram!(
            "server.vm.rollback_time",
            stage_started_at.elapsed(),
            "stage" => "memory_rollback"
        );

        stage_started_at = Instant::now();
        vlog::trace!("Rolling back precompiles_processor");
        self.state
            .precompiles_processor
            .rollback_to_timestamp(timestamp);
        metrics::histogram!(
            "server.vm.rollback_time",
            stage_started_at.elapsed(),
            "stage" => "precompiles_processor_rollback"
        );

        self.state.local_state = local_state;
        stage_started_at = Instant::now();
        self.bootloader_state.apply_snapshot(bootloader_state);
        metrics::histogram!(
            "server.vm.rollback_time",
            stage_started_at.elapsed(),
            "stage" => "apply_bootloader_snapshot"
        );
    }
}
