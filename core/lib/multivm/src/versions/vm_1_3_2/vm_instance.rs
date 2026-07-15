use std::{collections::VecDeque, convert::TryFrom, fmt::Debug};

use itertools::Itertools;
use zk_evm_1_3_3::{
    aux_structures::Timestamp,
    vm_state::{PrimitiveValue, VmLocalState, VmState},
    witness_trace::DummyTracer,
    zkevm_opcode_defs::{
        decoding::{AllowedPcOrImm, EncodingModeProduction, VmEncodingMode},
        definitions::RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
    },
};
use zksync_types::{
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    L1BatchNumber, H256, U256,
};

use crate::{
    glue::GlueInto,
    interface::{storage::WriteStorage, Call, TxExecutionStatus, VmEvent, VmExecutionLogs},
    versions::shared::{VmExecutionTrace, VmTrace},
    vm_1_3_2::{
        bootloader_state::BootloaderState,
        errors::{TxRevertReason, VmRevertReason, VmRevertReasonParsingResult},
        event_sink::InMemoryEventSink,
        events::merge_events,
        history_recorder::{HistoryEnabled, HistoryMode},
        memory::SimpleMemory,
        oracles::{
            decommitter::DecommitterOracle,
            precompile::PrecompilesProcessorWithHistory,
            storage::StorageOracle,
            tracer::{
                BootloaderTracer, ExecutionEndTracer, OneTxTracer, PendingRefundTracer,
                PubdataSpentTracer, StorageInvocationTracer, TransactionResultTracer,
                ValidationError, ValidationTracer, ValidationTracerParams,
            },
            OracleWithHistory,
        },
        utils::{
            calculate_computational_gas_used, dump_memory_page_using_primitive_value,
            precompile_calls_count_after_timestamp, StorageLogQuery,
        },
        vm_with_bootloader::{
            BootloaderJobType, DerivedBlockContext, TxExecutionMode, BOOTLOADER_HEAP_PAGE,
            OPERATOR_REFUNDS_OFFSET,
        },
    },
};

pub type ZkSyncVmState<S, H> = VmState<
    StorageOracle<S, H>,
    SimpleMemory<H>,
    InMemoryEventSink<H>,
    PrecompilesProcessorWithHistory<false, H>,
    DecommitterOracle<S, false, H>,
    DummyTracer,
>;
pub const MAX_MEM_SIZE_BYTES: u32 = 16777216; // 2^24

// Arbitrary space in memory closer to the end of the page
pub const RESULT_SUCCESS_FIRST_SLOT: u32 =
    (MAX_MEM_SIZE_BYTES - (MAX_TXS_IN_BLOCK as u32) * 32) / 32;
// The slot that is used for tracking vm hooks
pub const VM_HOOK_POSITION: u32 = RESULT_SUCCESS_FIRST_SLOT - 1;
pub const VM_HOOK_PARAMS_COUNT: u32 = 2;
pub const VM_HOOK_PARAMS_START_POSITION: u32 = VM_HOOK_POSITION - VM_HOOK_PARAMS_COUNT;

pub(crate) fn get_vm_hook_params<H: HistoryMode>(memory: &SimpleMemory<H>) -> Vec<U256> {
    memory.dump_page_content_as_u256_words(
        BOOTLOADER_HEAP_PAGE,
        VM_HOOK_PARAMS_START_POSITION..VM_HOOK_PARAMS_START_POSITION + VM_HOOK_PARAMS_COUNT,
    )
}

#[derive(Debug)]
pub struct VmInstance<S: WriteStorage, H: HistoryMode> {
    pub gas_limit: u32,
    pub state: ZkSyncVmState<S, H>,
    pub execution_mode: TxExecutionMode,
    pub block_context: DerivedBlockContext,
    pub(crate) bootloader_state: BootloaderState,

    pub snapshots: VecDeque<VmSnapshot>,
}

/// This structure stores data that accumulates during the VM run.
#[derive(Debug, PartialEq)]
pub struct VmExecutionResult {
    pub events: Vec<VmEvent>,
    pub storage_log_queries: Vec<StorageLogQuery>,
    pub used_contract_hashes: Vec<U256>,
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
    pub return_data: Vec<u8>,

    /// Value denoting the amount of gas spent within VM invocation.
    /// Note that return value represents the difference between the amount of gas
    /// available to VM before and after execution.
    ///
    /// It means, that depending on the context, `gas_used` may represent different things.
    /// If VM is continuously invoked and interrupted after each tx, this field may represent the
    /// amount of gas spent by a single transaction.
    ///
    /// To understand, which value does `gas_used` represent, see the documentation for the method
    /// that you use to get `VmExecutionResult` object.
    ///
    /// Side note: this may sound confusing, but this arises from the nature of the bootloader: for it,
    /// processing multiple transactions is a single action. We *may* intrude and stop VM once transaction
    /// is executed, but it's not enforced. So best we can do is to calculate the amount of gas before and
    /// after the invocation, leaving the interpretation of this value to the user.
    pub gas_used: u32,
    pub gas_remaining: u32,
    /// This value also depends on the context, the same as `gas_used`.
    pub computational_gas_used: u32,
    pub contracts_used: usize,
    pub revert_reason: Option<VmRevertReasonParsingResult>,
    pub trace: VmTrace,
    pub total_log_queries: usize,
    pub cycles_used: u32,
}

impl VmExecutionResult {
    pub fn error_message(&self) -> Option<String> {
        self.revert_reason
            .as_ref()
            .map(|result| result.revert_reason.to_string())
    }
}

#[derive(Debug, PartialEq)]
pub struct VmBlockResult {
    /// Result for the whole block execution.
    pub full_result: VmExecutionResult,
    /// Result for the block tip execution.
    pub block_tip_result: VmPartialExecutionResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VmPartialExecutionResult {
    pub logs: VmExecutionLogs,
    pub revert_reason: Option<TxRevertReason>,
    pub contracts_used: usize,
    pub cycles_used: u32,
    pub computational_gas_used: u32,
    pub gas_remaining: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VmTxExecutionResult {
    pub status: TxExecutionStatus,
    pub result: VmPartialExecutionResult,
    pub call_traces: Vec<Call>,
    // Gas refunded to the user at the end of the transaction
    pub gas_refunded: u32,
    // Gas proposed by the operator to be refunded, before the postOp call.
    // This value is needed to correctly recover memory of the bootloader.
    pub operator_suggested_refund: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VmExecutionStopReason {
    VmFinished,
    TracerRequestedStop,
}

use super::vm_with_bootloader::MAX_TXS_IN_BLOCK;
use crate::vm_1_3_2::utils::VmExecutionResult as NewVmExecutionResult;

fn vm_may_have_ended_inner<S: WriteStorage, H: HistoryMode>(
    vm: &ZkSyncVmState<S, H>,
) -> Option<NewVmExecutionResult> {
    let execution_has_ended = vm.execution_has_ended();

    let r1 = vm.local_state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];
    let current_address = vm.local_state.callstack.get_current_stack().this_address;

    let outer_eh_location = <EncodingModeProduction as VmEncodingMode<8>>::PcOrImm::MAX.as_u64();
    match (
        execution_has_ended,
        vm.local_state.callstack.get_current_stack().pc.as_u64(),
    ) {
        (true, 0) => {
            let returndata = dump_memory_page_using_primitive_value(&vm.memory, r1);

            Some(NewVmExecutionResult::Ok(returndata))
        }
        (false, _) => None,
        (true, l) if l == outer_eh_location => {
            // check `r1,r2,r3`
            if vm.local_state.flags.overflow_or_less_than_flag {
                Some(NewVmExecutionResult::Panic)
            } else {
                let returndata = dump_memory_page_using_primitive_value(&vm.memory, r1);
                Some(NewVmExecutionResult::Revert(returndata))
            }
        }
        (_, a) => Some(NewVmExecutionResult::MostLikelyDidNotFinish(
            current_address,
            a as u16,
        )),
    }
}

// This method returns `VmExecutionResult` struct, but some of the fields are left empty.
//
// `gas_before` argument is used to calculate the amount of gas spent by transaction.
// It is required because the same VM instance is continuously used to apply several transactions.
fn vm_may_have_ended<H: HistoryMode, S: WriteStorage>(
    vm: &VmInstance<S, H>,
    gas_before: u32,
) -> Option<VmExecutionResult> {
    let basic_execution_result = vm_may_have_ended_inner(&vm.state)?;

    let gas_remaining = vm.gas_remaining();
    let gas_used = gas_before.checked_sub(gas_remaining).expect("underflow");

    match basic_execution_result {
        NewVmExecutionResult::Ok(data) => {
            Some(VmExecutionResult {
                // The correct `events` value for this field should be set separately
                // later on based on the information inside the `event_sink` oracle.
                events: vec![],
                storage_log_queries: vm.state.storage.get_final_log_queries(),
                used_contract_hashes: vm.get_used_contracts(),
                l2_to_l1_logs: vec![],
                return_data: data,
                gas_used,
                gas_remaining,
                // The correct `computational_gas_used` value for this field should be set separately later.
                computational_gas_used: 0,
                contracts_used: vm
                    .state
                    .decommittment_processor
                    .get_used_bytecode_hashes()
                    .len(),
                revert_reason: None,
                trace: VmTrace::ExecutionTrace(VmExecutionTrace::default()),
                total_log_queries: vm.state.event_sink.get_log_queries()
                    + vm.state.precompiles_processor.get_timestamp_history().len()
                    + vm.state.storage.get_final_log_queries().len(),
                cycles_used: vm.state.local_state.monotonic_cycle_counter,
            })
        }
        NewVmExecutionResult::Revert(data) => {
            let revert_reason = VmRevertReasonParsingResult::new(
                TxRevertReason::parse_error(data.as_slice()),
                data,
            );

            // Check if error indicates a bug in server/vm/bootloader.
            if matches!(
                revert_reason.revert_reason,
                TxRevertReason::UnexpectedVMBehavior(_)
            ) {
                tracing::error!(
                    "Observed error that should never happen: {:?}. Full VM data: {:?}",
                    revert_reason,
                    vm
                );
            }

            Some(VmExecutionResult {
                events: vec![],
                storage_log_queries: vm.state.storage.get_final_log_queries(),
                used_contract_hashes: vm.get_used_contracts(),
                l2_to_l1_logs: vec![],
                return_data: vec![],
                gas_used,
                gas_remaining,
                // The correct `computational_gas_used` value for this field should be set separately later.
                computational_gas_used: 0,
                contracts_used: vm
                    .state
                    .decommittment_processor
                    .get_used_bytecode_hashes()
                    .len(),
                revert_reason: Some(revert_reason),
                trace: VmTrace::ExecutionTrace(VmExecutionTrace::default()),
                total_log_queries: vm.state.event_sink.get_log_queries()
                    + vm.state.precompiles_processor.get_timestamp_history().len()
                    + vm.state.storage.get_final_log_queries().len(),
                cycles_used: vm.state.local_state.monotonic_cycle_counter,
            })
        }
        // Panic is effectively the same as Revert, but has different nature.
        NewVmExecutionResult::Panic => Some(VmExecutionResult {
            events: vec![],
            storage_log_queries: vec![],
            used_contract_hashes: vec![],
            l2_to_l1_logs: vec![],
            return_data: vec![],
            gas_used,
            gas_remaining,
            // The correct `computational_gas_used` value for this field should be set separately later.
            computational_gas_used: 0,
            contracts_used: vm
                .state
                .decommittment_processor
                .get_used_bytecode_hashes()
                .len(),
            revert_reason: Some(VmRevertReasonParsingResult {
                revert_reason: TxRevertReason::Unknown(VmRevertReason::VmError),
                original_data: vec![],
            }),
            trace: VmTrace::ExecutionTrace(VmExecutionTrace::default()),
            total_log_queries: vm.state.event_sink.get_log_queries()
                + vm.state.precompiles_processor.get_timestamp_history().len()
                + vm.state.storage.get_final_log_queries().len(),
            cycles_used: vm.state.local_state.monotonic_cycle_counter,
        }),
        NewVmExecutionResult::MostLikelyDidNotFinish(_, _) => {
            // The execution has not ended yet. It should either continue
            // or throw Out-of-gas error.
            None
        }
    }
}

/// A snapshot of the VM that holds enough information to
/// rollback the VM to some historical state.
#[derive(Debug, Clone)]
pub struct VmSnapshot {
    local_state: VmLocalState,
    bootloader_state: BootloaderState,
}

impl<H: HistoryMode, S: WriteStorage> VmInstance<S, H> {
    pub(crate) fn is_bytecode_known(&self, bytecode_hash: &H256) -> bool {
        self.state
            .storage
            .storage
            .get_ptr()
            .borrow_mut()
            .is_bytecode_known(bytecode_hash)
    }
    fn has_ended(&self) -> bool {
        match vm_may_have_ended_inner(&self.state) {
            None | Some(NewVmExecutionResult::MostLikelyDidNotFinish(_, _)) => false,
            Some(
                NewVmExecutionResult::Ok(_)
                | NewVmExecutionResult::Revert(_)
                | NewVmExecutionResult::Panic,
            ) => true,
        }
    }

    fn revert_reason(&self) -> Option<VmRevertReasonParsingResult> {
        match vm_may_have_ended_inner(&self.state) {
            None
            | Some(
                NewVmExecutionResult::MostLikelyDidNotFinish(_, _) | NewVmExecutionResult::Ok(_),
            ) => None,
            Some(NewVmExecutionResult::Revert(data)) => {
                let revert_reason = VmRevertReasonParsingResult::new(
                    TxRevertReason::parse_error(data.as_slice()),
                    data,
                );

                // Check if error indicates a bug in server/vm/bootloader.
                if matches!(
                    revert_reason.revert_reason,
                    TxRevertReason::UnexpectedVMBehavior(_)
                ) {
                    tracing::error!(
                        "Observed error that should never happen: {:?}. Full VM data: {:?}",
                        revert_reason,
                        self
                    );
                }

                Some(revert_reason)
            }
            Some(NewVmExecutionResult::Panic) => Some(VmRevertReasonParsingResult {
                revert_reason: TxRevertReason::Unknown(VmRevertReason::VmError),
                original_data: vec![],
            }),
        }
    }

    /// Returns the amount of gas remaining to the VM.
    /// Note that this *does not* correspond to the gas limit of a transaction.
    /// To calculate the amount of gas spent by transaction, you should call this method before and after
    /// the execution, and subtract these values.
    ///
    /// Note: this method should only be called when either transaction is fully completed or VM completed
    /// its execution. Remaining gas value is read from the current stack frame, so if you'll attempt to
    /// read it during the transaction execution, you may receive invalid value.
    pub(crate) fn gas_remaining(&self) -> u32 {
        self.state.local_state.callstack.current.ergs_remaining
    }

    /// Returns the amount of gas consumed by the VM so far (based on the `gas_limit` provided
    /// to initiate the virtual machine).
    ///
    /// Note: this method should only be called when either transaction is fully completed or VM completed
    /// its execution. Remaining gas value is read from the current stack frame, so if you'll attempt to
    /// read it during the transaction execution, you may receive invalid value.
    pub fn gas_consumed(&self) -> u32 {
        self.gas_limit
            .checked_sub(self.gas_remaining())
            .expect("underflow")
    }

    pub(crate) fn collect_events_and_l1_logs_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> (Vec<VmEvent>, Vec<L2ToL1Log>) {
        let (raw_events, l1_messages) = self
            .state
            .event_sink
            .get_events_and_l2_l1_logs_after_timestamp(from_timestamp);
        let events = merge_events(raw_events)
            .into_iter()
            .map(|e| e.into_vm_event(L1BatchNumber(self.block_context.context.block_number)))
            .collect();
        (
            events,
            l1_messages.into_iter().map(GlueInto::glue_into).collect(),
        )
    }

    fn collect_execution_logs_after_timestamp(&self, from_timestamp: Timestamp) -> VmExecutionLogs {
        let storage_logs = self
            .state
            .storage
            .storage_log_queries_after_timestamp(from_timestamp)
            .to_vec();
        let storage_logs_count = storage_logs.len();
        let storage_logs = storage_logs.iter().map(|x| **x).collect_vec();

        let (events, l2_to_l1_logs) =
            self.collect_events_and_l1_logs_after_timestamp(from_timestamp);

        let log_queries = self
            .state
            .event_sink
            .log_queries_after_timestamp(from_timestamp);

        let precompile_calls_count = precompile_calls_count_after_timestamp(
            self.state.precompiles_processor.timestamp_history.inner(),
            from_timestamp,
        );
        VmExecutionLogs {
            storage_logs: storage_logs.into_iter().map(GlueInto::glue_into).collect(),
            events,
            user_l2_to_l1_logs: l2_to_l1_logs.into_iter().map(UserL2ToL1Log).collect(),
            total_log_queries_count: storage_logs_count
                + log_queries.len()
                + precompile_calls_count,
            system_l2_to_l1_logs: vec![],
        }
    }

    /// Executes VM until the end or tracer says to stop.
    /// Returns a tuple of `VmExecutionStopReason` and the size of the refund proposed by the operator
    fn execute_with_custom_tracer_and_refunds<
        T: ExecutionEndTracer<H>
            + PendingRefundTracer<H>
            + PubdataSpentTracer<H>
            + StorageInvocationTracer<H>,
    >(
        &mut self,
        tracer: &mut T,
    ) -> (VmExecutionStopReason, u32) {
        let mut operator_refund = None;
        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let gas_remaining_before = self.gas_remaining();
        let spent_pubdata_counter_before = self.state.local_state.spent_pubdata_counter;

        loop {
            // Sanity check: we should never reach the maximum value, because then we won't be able to process the next cycle.
            assert_ne!(
                self.state.local_state.monotonic_cycle_counter,
                u32::MAX,
                "VM reached maximum possible amount of cycles. Vm state: {:?}",
                self.state
            );

            let timestamp_before_cycle = self.state.local_state.timestamp;
            self.state
                .cycle(tracer)
                .expect("Failed execution VM cycle.");

            if self.has_ended() {
                return (
                    VmExecutionStopReason::VmFinished,
                    operator_refund.unwrap_or_default(),
                );
            }

            // This means that the bootloader has informed the system (usually via `VMHooks`) - that some gas
            // should be refunded back (see `askOperatorForRefund` in `bootloader.yul` for details).
            if let Some(bootloader_refund) = tracer.requested_refund() {
                assert!(
                    operator_refund.is_none(),
                    "Operator was asked for refund two times"
                );

                let gas_spent_on_pubdata = tracer.gas_spent_on_pubdata(&self.state.local_state)
                    - spent_pubdata_counter_before;
                let tx_body_refund =
                    self.tx_body_refund(timestamp_initial, bootloader_refund, gas_spent_on_pubdata);

                if tx_body_refund < bootloader_refund {
                    tracing::error!(
                        "Suggested tx body refund is less than bootloader refund. Tx body refund: {}, bootloader refund: {}",
                        tx_body_refund,
                        bootloader_refund
                    );
                }

                let refund_to_propose = tx_body_refund
                    + self.block_overhead_refund(
                        timestamp_initial,
                        gas_remaining_before,
                        gas_spent_on_pubdata,
                    );

                let current_tx_index = self.bootloader_state.tx_to_execute() - 1;
                let refund_slot = OPERATOR_REFUNDS_OFFSET + current_tx_index;

                // Writing the refund into memory
                self.state.memory.memory.write_to_memory(
                    BOOTLOADER_HEAP_PAGE as usize,
                    refund_slot,
                    PrimitiveValue {
                        value: refund_to_propose.into(),
                        is_pointer: false,
                    },
                    Timestamp(timestamp_before_cycle),
                );
                operator_refund = Some(refund_to_propose);
                tracer.set_refund_as_done();

                let tx_gas_limit = self.get_tx_gas_limit(current_tx_index);

                if tx_gas_limit < bootloader_refund {
                    tracing::error!(
                        "Tx gas limit is less than bootloader refund. Tx gas limit: {}, bootloader refund: {}",
                        tx_gas_limit,
                        bootloader_refund
                    );
                }
                if tx_gas_limit < refund_to_propose {
                    tracing::error!(
                        "Tx gas limit is less than operator refund. Tx gas limit: {}, operator refund: {}",
                        tx_gas_limit,
                        refund_to_propose
                    );
                }
            }

            tracer.set_missed_storage_invocations(
                self.state
                    .storage
                    .storage
                    .get_ptr()
                    .borrow()
                    .missed_storage_invocations(),
            );

            if tracer.should_stop_execution() {
                return (
                    VmExecutionStopReason::TracerRequestedStop,
                    operator_refund.unwrap_or_default(),
                );
            }
        }
    }

    // Executes VM until the end or tracer says to stop.
    pub(crate) fn execute_with_custom_tracer<
        T: ExecutionEndTracer<H>
            + PendingRefundTracer<H>
            + PubdataSpentTracer<H>
            + StorageInvocationTracer<H>,
    >(
        &mut self,
        tracer: &mut T,
    ) -> VmExecutionStopReason {
        self.execute_with_custom_tracer_and_refunds(tracer).0
    }

    /// Executes the VM until the end of the next transaction.
    /// Panics if there are no new transactions in bootloader.
    /// Internally uses the OneTxTracer to stop the VM when the last opcode from the transaction is reached.
    // Err when transaction is rejected.
    // `Ok(status: TxExecutionStatus::Success)` when the transaction succeeded
    // `Ok(status: TxExecutionStatus::Failure)` when the transaction failed.
    // Note that failed transactions are considered properly processed and are included in blocks
    pub fn execute_next_tx(
        &mut self,
        validation_computational_gas_limit: u32,
        with_call_tracer: bool,
    ) -> Result<VmTxExecutionResult, TxRevertReason> {
        let tx_index = self.bootloader_state.next_unexecuted_tx() as u32;

        let mut tx_tracer: OneTxTracer<H> =
            OneTxTracer::new(validation_computational_gas_limit, with_call_tracer);

        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;
        let gas_remaining_before = self.gas_remaining();
        let spent_pubdata_counter_before = self.state.local_state.spent_pubdata_counter;

        let (stop_reason, operator_suggested_refund) =
            self.execute_with_custom_tracer_and_refunds(&mut tx_tracer);
        match stop_reason {
            VmExecutionStopReason::VmFinished => {
                // Bootloader resulted in panic or revert, this means either the transaction is rejected
                // (e.g. not enough fee or incorrect signature) or bootloader is out of gas.

                // Collect generated events to show bootloader debug logs.
                let _ = self.collect_events_and_l1_logs_after_timestamp(timestamp_initial);

                let error = if tx_tracer.is_bootloader_out_of_gas() {
                    TxRevertReason::BootloaderOutOfGas
                } else {
                    self.revert_reason()
                        .expect("vm ended execution prematurely, but no revert reason is given")
                        .revert_reason
                };
                Err(error)
            }
            VmExecutionStopReason::TracerRequestedStop => {
                if tx_tracer.tx_has_been_processed() {
                    let tx_execution_status =
                        TxExecutionStatus::from_has_failed(tx_has_failed(&self.state, tx_index));
                    let vm_execution_logs =
                        self.collect_execution_logs_after_timestamp(timestamp_initial);

                    let computational_gas_used = calculate_computational_gas_used(
                        self,
                        &tx_tracer,
                        gas_remaining_before,
                        spent_pubdata_counter_before,
                    );

                    Ok(VmTxExecutionResult {
                        gas_refunded: tx_tracer.refund_gas,
                        operator_suggested_refund,
                        status: tx_execution_status,
                        result: VmPartialExecutionResult {
                            logs: vm_execution_logs,
                            // If there is a revert Err is already returned above.
                            revert_reason: None,
                            // getting contracts used during this transaction
                            // at least for now the number returned here is always <= to the number
                            // of the code hashes actually used by the transaction, since it might have
                            // reused bytecode hashes from some of the previous ones.
                            contracts_used: self
                                .state
                                .decommittment_processor
                                .get_decommitted_bytecodes_after_timestamp(timestamp_initial),
                            cycles_used: self.state.local_state.monotonic_cycle_counter
                                - cycles_initial,
                            computational_gas_used,
                            gas_remaining: self.gas_remaining(),
                        },
                        call_traces: tx_tracer.call_traces(),
                    })
                } else if tx_tracer.validation_run_out_of_gas() {
                    Err(TxRevertReason::ValidationFailed(VmRevertReason::General {
                        msg: format!(
                            "Took too many computational gas, allowed limit: {}",
                            validation_computational_gas_limit
                        ),
                        data: vec![],
                    }))
                } else {
                    // VM ended up in state
                    // `stop_reason == VmExecutionStopReason::TracerRequestedStop && !tx_tracer.tx_has_been_processed() && !tx_tracer.validation_run_out_of_gas()`.
                    // It means that bootloader successfully finished its execution without executing the transaction.
                    // It is an unexpected situation.
                    panic!("VM successfully finished executing bootloader but transaction wasn't executed");
                }
            }
        }
    }

    /// Returns full VM result and partial result produced within the current execution.
    pub fn execute_till_block_end(&mut self, job_type: BootloaderJobType) -> VmBlockResult {
        self.execute_till_block_end_with_tracer(
            job_type,
            &mut TransactionResultTracer::new(self.execution_mode.invocation_limit(), false),
        )
    }

    pub fn execute_till_block_end_with_call_tracer(
        &mut self,
        job_type: BootloaderJobType,
    ) -> VmBlockResult {
        let mut tracer = TransactionResultTracer::new(self.execution_mode.invocation_limit(), true);
        let mut block_result = self.execute_till_block_end_with_tracer(job_type, &mut tracer);
        block_result.full_result.trace = VmTrace::CallTrace(tracer.call_trace().unwrap());
        block_result
    }

    fn execute_till_block_end_with_tracer(
        &mut self,
        job_type: BootloaderJobType,
        tx_result_tracer: &mut TransactionResultTracer<H>,
    ) -> VmBlockResult {
        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;
        let gas_remaining_before = self.gas_remaining();
        let spent_pubdata_counter_before = self.state.local_state.spent_pubdata_counter;

        let stop_reason = self.execute_with_custom_tracer(tx_result_tracer);
        match stop_reason {
            VmExecutionStopReason::VmFinished => {
                let mut full_result = vm_may_have_ended(self, gas_remaining_before).unwrap();

                let computational_gas_used = calculate_computational_gas_used(
                    self,
                    tx_result_tracer,
                    gas_remaining_before,
                    spent_pubdata_counter_before,
                );

                if job_type == BootloaderJobType::TransactionExecution
                    && tx_has_failed(&self.state, 0)
                    && full_result.revert_reason.is_none()
                {
                    let revert_reason = tx_result_tracer
                        .revert_reason
                        .clone()
                        .map(|reason| {
                            let vm_revert_reason = VmRevertReason::try_from(reason.as_slice())
                                .unwrap_or_else(|_| VmRevertReason::Unknown {
                                    function_selector: vec![],
                                    data: reason.clone(),
                                });

                            VmRevertReasonParsingResult {
                                revert_reason: TxRevertReason::TxReverted(vm_revert_reason),
                                original_data: reason,
                            }
                        })
                        .unwrap_or_else(|| VmRevertReasonParsingResult {
                            revert_reason: TxRevertReason::TxReverted(VmRevertReason::General {
                                msg: "Transaction reverted with empty reason. Possibly out of gas"
                                    .to_string(),
                                data: vec![],
                            }),
                            original_data: vec![],
                        });

                    full_result.revert_reason = Some(revert_reason);
                }

                let block_tip_result = VmPartialExecutionResult {
                    logs: self.collect_execution_logs_after_timestamp(timestamp_initial),
                    revert_reason: full_result.revert_reason.clone().map(|r| r.revert_reason),
                    contracts_used: self
                        .state
                        .decommittment_processor
                        .get_decommitted_bytecodes_after_timestamp(timestamp_initial),
                    cycles_used: self.state.local_state.monotonic_cycle_counter - cycles_initial,
                    computational_gas_used,
                    gas_remaining: self.gas_remaining(),
                };

                // Collecting `block_tip_result` needs logs with timestamp, so we drain events for the `full_result`
                // after because draining will drop timestamps.
                let (raw_events, l1_messages) = self.state.event_sink.flatten();
                full_result.events = merge_events(raw_events)
                    .into_iter()
                    .map(|e| {
                        e.into_vm_event(L1BatchNumber(self.block_context.context.block_number))
                    })
                    .collect();
                full_result.l2_to_l1_logs =
                    l1_messages.into_iter().map(GlueInto::glue_into).collect();
                full_result.computational_gas_used = block_tip_result.computational_gas_used;
                VmBlockResult {
                    full_result,
                    block_tip_result,
                }
            }
            VmExecutionStopReason::TracerRequestedStop => {
                if tx_result_tracer.is_limit_reached() {
                    VmBlockResult {
                        // Normally tracer should never stop, but if it's transaction call and it consumes
                        // too much requests to memory, we stop execution and return error.
                        full_result: VmExecutionResult {
                            events: vec![],
                            storage_log_queries: vec![],
                            used_contract_hashes: vec![],
                            l2_to_l1_logs: vec![],
                            return_data: vec![],
                            gas_used: 0,
                            gas_remaining: 0,
                            computational_gas_used: 0,
                            contracts_used: 0,
                            revert_reason: Some(VmRevertReasonParsingResult {
                                revert_reason: TxRevertReason::MissingInvocationLimitReached,
                                original_data: vec![],
                            }),
                            trace: VmTrace::ExecutionTrace(VmExecutionTrace::default()),
                            total_log_queries: 0,
                            cycles_used: 0,
                        },
                        block_tip_result: VmPartialExecutionResult {
                            logs: Default::default(),
                            revert_reason: Some(TxRevertReason::MissingInvocationLimitReached),
                            contracts_used: 0,
                            cycles_used: 0,
                            computational_gas_used: 0,
                            gas_remaining: 0,
                        },
                    }
                } else {
                    unreachable!(
                        "Tracer should never stop execution, except MissingInvocationLimitReached"
                    );
                }
            }
        }
    }

    /// Unlike `execute_till_block_end` methods returns only result for the block tip execution.
    pub fn execute_block_tip(&mut self) -> VmPartialExecutionResult {
        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;
        let gas_remaining_before = self.gas_remaining();
        let spent_pubdata_counter_before = self.state.local_state.spent_pubdata_counter;
        let mut bootloader_tracer: BootloaderTracer<H> = BootloaderTracer::default();

        let stop_reason = self.execute_with_custom_tracer(&mut bootloader_tracer);
        let revert_reason = match stop_reason {
            VmExecutionStopReason::VmFinished => {
                // Bootloader panicked or reverted.
                let revert_reason = if bootloader_tracer.is_bootloader_out_of_gas() {
                    TxRevertReason::BootloaderOutOfGas
                } else {
                    self.revert_reason()
                        .expect("vm ended execution prematurely, but no revert reason is given")
                        .revert_reason
                };
                Some(revert_reason)
            }
            VmExecutionStopReason::TracerRequestedStop => {
                // Bootloader finished successfully.
                None
            }
        };

        let computational_gas_used = calculate_computational_gas_used(
            self,
            &bootloader_tracer,
            gas_remaining_before,
            spent_pubdata_counter_before,
        );
        VmPartialExecutionResult {
            logs: self.collect_execution_logs_after_timestamp(timestamp_initial),
            revert_reason,
            contracts_used: self
                .state
                .decommittment_processor
                .get_decommitted_bytecodes_after_timestamp(timestamp_initial),
            cycles_used: self.state.local_state.monotonic_cycle_counter - cycles_initial,
            computational_gas_used,
            gas_remaining: self.gas_remaining(),
        }
    }

    pub fn execute_validation(
        &mut self,
        validation_params: ValidationTracerParams,
    ) -> Result<(), ValidationError> {
        let mut validation_tracer: ValidationTracer<S, H> = ValidationTracer::new(
            self.state.storage.storage.inner().get_ptr(),
            validation_params,
        );

        let stop_reason = self.execute_with_custom_tracer(&mut validation_tracer);

        match (stop_reason, validation_tracer.validation_error) {
            (VmExecutionStopReason::VmFinished, _) => {
                // The tx should only end in case of a revert, so it is safe to unwrap here
                Err(ValidationError::FailedTx(self.revert_reason().unwrap()))
            }
            (VmExecutionStopReason::TracerRequestedStop, Some(err)) => {
                Err(ValidationError::ViolatedRule(err))
            }
            (VmExecutionStopReason::TracerRequestedStop, None) => Ok(()),
        }
    }

    /// Returns the keys of contracts that are already loaded (known) by bootloader.
    pub(crate) fn get_used_contracts(&self) -> Vec<U256> {
        self.state
            .decommittment_processor
            .decommitted_code_hashes
            .inner()
            .keys()
            .cloned()
            .collect()
    }

    pub fn number_of_updated_storage_slots(&self) -> usize {
        self.state
            .storage
            .storage
            .inner()
            .get_ptr()
            .borrow_mut()
            .modified_storage_keys()
            .len()
    }
}

impl<S: WriteStorage> VmInstance<S, HistoryEnabled> {
    /// Saves the snapshot of the current state of the VM that can be used
    /// to roll back its state later on.
    pub fn save_current_vm_as_snapshot(&mut self) {
        self.snapshots.push_back(VmSnapshot {
            // Vm local state contains O(1) various parameters (registers/etc).
            // The only "expensive" copying here is copying of the call stack.
            // It will take `O(callstack_depth)` to copy it.
            // So it is generally recommended to get snapshots of the bootloader frame,
            // where the depth is 1.
            local_state: self.state.local_state.clone(),
            bootloader_state: self.bootloader_state.clone(),
        });
    }

    fn rollback_to_snapshot(&mut self, snapshot: VmSnapshot) {
        let VmSnapshot {
            local_state,
            bootloader_state,
        } = snapshot;

        let timestamp = Timestamp(local_state.timestamp);

        tracing::trace!("Rolling back decomitter");
        self.state
            .decommittment_processor
            .rollback_to_timestamp(timestamp);

        tracing::trace!("Rolling back event_sink");
        self.state.event_sink.rollback_to_timestamp(timestamp);

        tracing::trace!("Rolling back storage");
        self.state.storage.rollback_to_timestamp(timestamp);

        tracing::trace!("Rolling back memory");
        self.state.memory.rollback_to_timestamp(timestamp);

        tracing::trace!("Rolling back precompiles_processor");
        self.state
            .precompiles_processor
            .rollback_to_timestamp(timestamp);
        self.state.local_state = local_state;
        self.bootloader_state = bootloader_state;
    }

    /// Rollbacks the state of the VM to the state of the latest snapshot.
    /// Removes that snapshot from the list.
    pub fn rollback_to_latest_snapshot_popping(&mut self) {
        let snapshot = self.snapshots.pop_back().unwrap();
        self.rollback_to_snapshot(snapshot);
    }

    /// Removes the latest snapshot without rolling it back.
    /// This function expects that there is at least one snapshot present.
    pub fn pop_snapshot_no_rollback(&mut self) {
        self.snapshots.pop_back();
    }

    /// Removes the earliest snapshot without rolling it back.
    /// This function expects that there is at least one snapshot present.
    pub fn pop_front_snapshot_no_rollback(&mut self) {
        self.snapshots.pop_front();
    }
}

// Reads the bootloader memory and checks whether the execution step of the transaction
// has failed.
pub(crate) fn tx_has_failed<H: HistoryMode, S: WriteStorage>(
    state: &ZkSyncVmState<S, H>,
    tx_id: u32,
) -> bool {
    let mem_slot = RESULT_SUCCESS_FIRST_SLOT + tx_id;
    let mem_value = state
        .memory
        .read_slot(BOOTLOADER_HEAP_PAGE as usize, mem_slot as usize)
        .value;

    mem_value == U256::zero()
}
