use zk_evm::aux_structures::Timestamp;
use zksync_state::WriteStorage;

use crate::old_vm::history_recorder::HistoryMode;
use crate::old_vm::utils::{vm_may_have_ended_inner, VmExecutionResult};
use crate::tracers::traits::{ExecutionEndTracer, ExecutionProcessing, VmTracer};
use crate::tracers::RefundsTracer;
use crate::tracers::{DefaultExecutionTracer, ExecutionMode};
use crate::types::outputs::VmExecutionResultAndLogs;
use crate::vm::Vm;
use crate::VmExecutionStopReason;

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    /// Executes the next transaction in the queue with the given tracers.
    pub(crate) fn inspect_next_tx_inner(
        &mut self,
        tracers: Vec<Box<dyn VmTracer<S, H>>>,
    ) -> VmExecutionResultAndLogs {
        // Move the pointer to the next transaction
        self.bootloader_state.move_tx_to_execute_pointer();
        let (_, result) = self.inspect_and_collect_results(tracers, ExecutionMode::OneTx, true);
        result
    }

    /// Execute VM with given traces until the stop reason is reached.
    /// Collect the result from the default tracers.
    pub(crate) fn inspect_and_collect_results(
        &mut self,
        tracers: Vec<Box<dyn VmTracer<S, H>>>,
        execution_mode: ExecutionMode,
        with_refund_tracer: bool,
    ) -> (VmExecutionStopReason, VmExecutionResultAndLogs) {
        let refund_tracers = if with_refund_tracer {
            Some(RefundsTracer::new(self.batch_env.clone()))
        } else {
            None
        };
        let mut tx_tracer: DefaultExecutionTracer<S, H> = DefaultExecutionTracer::new(
            self.system_env.default_validation_computational_gas_limit,
            execution_mode,
            tracers,
            self.storage.clone(),
            refund_tracers,
        );

        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;
        let gas_remaining_before = self.gas_remaining();
        let spent_pubdata_counter_before = self.state.local_state.spent_pubdata_counter;

        let stop_reason = self.execute(&mut tx_tracer);

        let gas_remaining_after = self.gas_remaining();

        let logs = self.collect_execution_logs_after_timestamp(timestamp_initial);

        let statistics = self.get_statistics(
            timestamp_initial,
            cycles_initial,
            &tx_tracer,
            gas_remaining_before,
            gas_remaining_after,
            spent_pubdata_counter_before,
            logs.total_log_queries_count,
        );

        let result = tx_tracer.result_tracer.into_result();

        let refunds = if let Some(refund_tracer) = tx_tracer.refund_tracer {
            refund_tracer.get_refunds()
        } else {
            Default::default()
        };

        let mut result = VmExecutionResultAndLogs {
            result,
            logs,
            statistics,
            refunds,
        };

        for tracer in tx_tracer.custom_tracers.iter_mut() {
            tracer.save_results(&mut result);
        }
        (stop_reason, result)
    }

    /// Execute the batch with the given tracers without stops after each tx
    pub(crate) fn inspect_the_rest_of_the_batch(
        &mut self,
        tracers: Vec<Box<dyn VmTracer<S, H>>>,
    ) -> VmExecutionResultAndLogs {
        let (_stop_reason, result) =
            self.inspect_and_collect_results(tracers, ExecutionMode::Block, false);
        result
    }

    /// Execute vm with given tracers until the stop reason is reached.
    fn execute(&mut self, tracer: &mut DefaultExecutionTracer<S, H>) -> VmExecutionStopReason {
        tracer.initialize_tracer(&mut self.state);
        let result = loop {
            // Sanity check: we should never reach the maximum value, because then we won't be able to process the next cycle.
            assert_ne!(
                self.state.local_state.monotonic_cycle_counter,
                u32::MAX,
                "VM reached maximum possible amount of cycles. Vm state: {:?}",
                self.state
            );

            tracer.before_cycle(&mut self.state);
            self.state
                .cycle(tracer)
                .expect("Failed execution VM cycle.");

            tracer.after_cycle(&mut self.state, &mut self.bootloader_state);
            if self.has_ended() {
                break VmExecutionStopReason::VmFinished;
            }

            if tracer.should_stop_execution() {
                break VmExecutionStopReason::TracerRequestedStop;
            }
        };
        tracer.after_vm_execution(&mut self.state, &self.bootloader_state, result);
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
