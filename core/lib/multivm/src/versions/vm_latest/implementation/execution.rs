use std::mem;

use zk_evm_1_5_2::aux_structures::Timestamp;
use zksync_vm_interface::VmEvent;

use crate::{
    interface::{
        storage::WriteStorage,
        tracer::{TracerExecutionStatus, VmExecutionStopReason},
        VmExecutionMode, VmExecutionResultAndLogs,
    },
    vm_latest::{
        old_vm::utils::{vm_may_have_ended_inner, VmExecutionResult},
        tracers::{
            circuits_capacity::circuit_statistic_from_cycles, dispatcher::TracerDispatcher,
            DefaultExecutionTracer, PubdataTracer, RefundsTracer,
        },
        vm::Vm,
    },
    HistoryMode,
};

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(crate) fn inspect_inner(
        &mut self,
        dispatcher: &mut TracerDispatcher<S, H::Vm1_5_2>,
        execution_mode: VmExecutionMode,
        custom_pubdata_tracer: Option<PubdataTracer<S>>,
    ) -> VmExecutionResultAndLogs {
        let mut enable_refund_tracer = false;
        if let VmExecutionMode::OneTx = execution_mode {
            // Move the pointer to the next transaction
            self.bootloader_state.move_tx_to_execute_pointer();
            enable_refund_tracer = true;
        }

        let (_, result) = self.inspect_and_collect_results(
            dispatcher,
            execution_mode,
            enable_refund_tracer,
            custom_pubdata_tracer,
        );
        result
    }

    /// Execute VM with given traces until the stop reason is reached.
    /// Collect the result from the default tracers.
    fn inspect_and_collect_results(
        &mut self,
        dispatcher: &mut TracerDispatcher<S, H::Vm1_5_2>,
        execution_mode: VmExecutionMode,
        with_refund_tracer: bool,
        custom_pubdata_tracer: Option<PubdataTracer<S>>,
    ) -> (VmExecutionStopReason, VmExecutionResultAndLogs) {
        let refund_tracers = with_refund_tracer
            .then_some(RefundsTracer::new(self.batch_env.clone(), self.subversion));
        let mut tx_tracer: DefaultExecutionTracer<S, H::Vm1_5_2> = DefaultExecutionTracer::new(
            self.system_env.default_validation_computational_gas_limit,
            self.system_env
                .base_system_smart_contracts
                .evm_emulator
                .is_some(),
            execution_mode,
            mem::take(dispatcher),
            self.storage.clone(),
            refund_tracers,
            custom_pubdata_tracer.or_else(|| {
                Some(PubdataTracer::new(
                    self.batch_env.clone(),
                    execution_mode,
                    self.subversion,
                    None,
                ))
            }),
            self.subversion,
        );

        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;
        let gas_remaining_before = self.gas_remaining();

        let stop_reason = self.execute_with_default_tracer(&mut tx_tracer);

        let gas_remaining_after = self.gas_remaining();

        let logs = self.collect_execution_logs_after_timestamp(timestamp_initial);

        let (refunds, pubdata_published) = tx_tracer
            .refund_tracer
            .as_ref()
            .map(|x| (x.get_refunds(), x.pubdata_published()))
            .unwrap_or_default();

        let statistics = self.get_statistics(
            timestamp_initial,
            cycles_initial,
            gas_remaining_before,
            gas_remaining_after,
            pubdata_published,
            logs.total_log_queries_count,
            circuit_statistic_from_cycles(tx_tracer.circuits_tracer.statistics),
        );
        let result = tx_tracer.result_tracer.into_result();
        let factory_deps_marked_as_known = VmEvent::extract_bytecodes_marked_as_known(&logs.events);
        let dynamic_factory_deps = self.decommit_dynamic_bytecodes(factory_deps_marked_as_known);
        *dispatcher = tx_tracer.dispatcher;

        let result = VmExecutionResultAndLogs {
            result,
            logs,
            statistics,
            refunds,
            dynamic_factory_deps,
        };

        (stop_reason, result)
    }

    /// Execute vm with given tracers until the stop reason is reached.
    fn execute_with_default_tracer(
        &mut self,
        tracer: &mut DefaultExecutionTracer<S, H::Vm1_5_2>,
    ) -> VmExecutionStopReason {
        tracer.initialize_tracer(&mut self.state);
        let result = loop {
            // Sanity check: we should never reach the maximum value, because then we won't be able to process the next cycle.
            assert_ne!(
                self.state.local_state.monotonic_cycle_counter,
                u32::MAX,
                "VM reached maximum possible amount of cycles. Vm state: {:?}",
                self.state
            );

            self.state
                .cycle(tracer)
                .expect("Failed execution VM cycle.");

            if let TracerExecutionStatus::Stop(reason) =
                tracer.finish_cycle(&mut self.state, &mut self.bootloader_state)
            {
                break VmExecutionStopReason::TracerRequestedStop(reason);
            }
            if self.has_ended() {
                break VmExecutionStopReason::VmFinished;
            }
        };
        tracer.after_vm_execution(&mut self.state, &self.bootloader_state, result.clone());
        result
    }

    fn has_ended(&self) -> bool {
        match vm_may_have_ended_inner(&self.state) {
            None | Some(VmExecutionResult::MostLikelyDidNotFinish(_, _)) => false,
            Some(
                VmExecutionResult::Ok(_) | VmExecutionResult::Revert(_) | VmExecutionResult::Panic,
            ) => true,
        }
    }
}
