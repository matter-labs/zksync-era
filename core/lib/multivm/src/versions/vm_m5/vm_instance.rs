use std::{collections::VecDeque, convert::TryFrom, fmt::Debug};

use zk_evm_1_3_1::{
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
    L1BatchNumber, U256,
};

use crate::{
    glue::GlueInto,
    interface::{TxExecutionStatus, VmEvent, VmExecutionLogs},
    versions::shared::VmExecutionTrace,
    vm_m5::{
        bootloader_state::BootloaderState,
        errors::{TxRevertReason, VmRevertReason, VmRevertReasonParsingResult},
        event_sink::InMemoryEventSink,
        events::merge_events,
        memory::SimpleMemory,
        oracles::{
            decommitter::DecommitterOracle,
            precompile::PrecompilesProcessorWithHistory,
            storage::StorageOracle,
            tracer::{
                BootloaderTracer, ExecutionEndTracer, OneTxTracer, PendingRefundTracer,
                PubdataSpentTracer, TransactionResultTracer, ValidationError, ValidationTracer,
                ValidationTracerParams,
            },
            OracleWithHistory,
        },
        storage::Storage,
        utils::{
            collect_log_queries_after_timestamp, collect_storage_log_queries_after_timestamp,
            dump_memory_page_using_primitive_value, precompile_calls_count_after_timestamp,
            StorageLogQuery,
        },
        vm_with_bootloader::{
            BootloaderJobType, DerivedBlockContext, TxExecutionMode, BOOTLOADER_HEAP_PAGE,
            OPERATOR_REFUNDS_OFFSET,
        },
    },
};

pub type ZkSyncVmState<S> = VmState<
    StorageOracle<S>,
    SimpleMemory,
    InMemoryEventSink,
    PrecompilesProcessorWithHistory<false>,
    DecommitterOracle<S, false>,
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

pub(crate) fn get_vm_hook_params(memory: &SimpleMemory) -> Vec<U256> {
    memory.dump_page_content_as_u256_words(
        BOOTLOADER_HEAP_PAGE,
        VM_HOOK_PARAMS_START_POSITION..VM_HOOK_PARAMS_START_POSITION + VM_HOOK_PARAMS_COUNT,
    )
}

/// MultiVM-specific addition.
///
/// At different points in time, refunds were handled in a different way.
/// E.g., initially they were completely disabled.
///
/// This enum allows to execute blocks with the same VM but different support for refunds.
#[derive(Debug, Copy, Clone)]
pub enum MultiVmSubversion {
    /// Initial VM M5 version, refunds are fully disabled.
    V1,
    /// Refunds were enabled. ETH balance for bootloader address was marked as a free slot.
    V2,
}

#[derive(Debug)]
pub struct VmInstance<S: Storage> {
    pub gas_limit: u32,
    pub state: ZkSyncVmState<S>,
    pub execution_mode: TxExecutionMode,
    pub block_context: DerivedBlockContext,
    pub(crate) bootloader_state: BootloaderState,

    pub snapshots: VecDeque<VmSnapshot>,

    /// MultiVM-specific addition. See enum doc-comment for details.
    pub(crate) refund_state: MultiVmSubversion,
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
    pub contracts_used: usize,
    pub revert_reason: Option<VmRevertReasonParsingResult>,
    pub trace: VmExecutionTrace,
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
    pub gas_remaining: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VmTxExecutionResult {
    pub status: TxExecutionStatus,
    pub result: VmPartialExecutionResult,
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
use crate::vm_m5::utils::VmExecutionResult as NewVmExecutionResult;

fn vm_may_have_ended_inner<const B: bool, S: Storage>(
    vm: &VmState<
        StorageOracle<S>,
        SimpleMemory,
        InMemoryEventSink,
        PrecompilesProcessorWithHistory<B>,
        DecommitterOracle<S, B>,
        DummyTracer,
    >,
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
fn vm_may_have_ended<S: Storage>(vm: &VmInstance<S>, gas_before: u32) -> Option<VmExecutionResult> {
    let basic_execution_result = vm_may_have_ended_inner(&vm.state)?;

    let gas_remaining = vm.gas_remaining();
    let gas_used = gas_before - gas_remaining;

    match basic_execution_result {
        NewVmExecutionResult::Ok(data) => {
            Some(VmExecutionResult {
                // The correct `events` value for this field should be set separately
                // later on based on the information inside the `event_sink` oracle.
                events: vec![],
                storage_log_queries: vm.get_final_log_queries(),
                used_contract_hashes: vm.get_used_contracts(),
                l2_to_l1_logs: vec![],
                return_data: data,
                gas_used,
                gas_remaining,
                contracts_used: vm
                    .state
                    .decommittment_processor
                    .get_used_bytecode_hashes()
                    .len(),
                revert_reason: None,
                trace: VmExecutionTrace::default(),
                total_log_queries: vm.state.event_sink.get_log_queries()
                    + vm.state.precompiles_processor.get_timestamp_history().len()
                    + vm.get_final_log_queries().len(),
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
                storage_log_queries: vm.get_final_log_queries(),
                used_contract_hashes: vm.get_used_contracts(),
                l2_to_l1_logs: vec![],
                return_data: vec![],
                gas_used,
                gas_remaining,
                contracts_used: vm
                    .state
                    .decommittment_processor
                    .get_used_bytecode_hashes()
                    .len(),
                revert_reason: Some(revert_reason),
                trace: VmExecutionTrace::default(),
                total_log_queries: vm.state.event_sink.get_log_queries()
                    + vm.state.precompiles_processor.get_timestamp_history().len()
                    + vm.get_final_log_queries().len(),
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
            contracts_used: vm
                .state
                .decommittment_processor
                .get_used_bytecode_hashes()
                .len(),
            revert_reason: Some(VmRevertReasonParsingResult {
                revert_reason: TxRevertReason::Unknown(VmRevertReason::VmError),
                original_data: vec![],
            }),
            trace: VmExecutionTrace::default(),
            total_log_queries: vm.state.event_sink.get_log_queries()
                + vm.state.precompiles_processor.get_timestamp_history().len()
                + vm.get_final_log_queries().len(),
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

impl<S: Storage> VmInstance<S> {
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
        self.gas_limit - self.gas_remaining()
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
        let storage_logs = collect_storage_log_queries_after_timestamp(
            &self
                .state
                .storage
                .frames_stack
                .inner()
                .current_frame()
                .forward,
            from_timestamp,
        );
        let storage_logs_count = storage_logs.len();

        let (events, l2_to_l1_logs) =
            self.collect_events_and_l1_logs_after_timestamp(from_timestamp);

        let log_queries = collect_log_queries_after_timestamp(
            &self
                .state
                .event_sink
                .frames_stack
                .inner()
                .current_frame()
                .forward,
            from_timestamp,
        );

        let precompile_calls_count = precompile_calls_count_after_timestamp(
            self.state.precompiles_processor.timestamp_history.inner(),
            from_timestamp,
        );
        VmExecutionLogs {
            storage_logs: storage_logs.into_iter().map(GlueInto::glue_into).collect(),
            events,
            user_l2_to_l1_logs: l2_to_l1_logs.into_iter().map(UserL2ToL1Log).collect(),
            system_l2_to_l1_logs: vec![],
            total_log_queries_count: storage_logs_count
                + log_queries.len()
                + precompile_calls_count,
        }
    }

    // Returns a tuple of `VmExecutionStopReason` and the size of the refund proposed by the operator
    fn execute_with_custom_tracer_and_refunds<
        T: ExecutionEndTracer + PendingRefundTracer + PubdataSpentTracer,
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
            self.state.cycle(tracer);

            if self.has_ended() {
                return (
                    VmExecutionStopReason::VmFinished,
                    operator_refund.unwrap_or_default(),
                );
            }

            if let Some(bootloader_refund) = tracer.requested_refund() {
                assert!(
                    operator_refund.is_none(),
                    "Operator was asked for refund two times"
                );

                let refund_to_propose;
                let refund_slot;
                match self.refund_state {
                    MultiVmSubversion::V1 => {
                        refund_to_propose = bootloader_refund;
                        refund_slot =
                            OPERATOR_REFUNDS_OFFSET + self.bootloader_state.tx_to_execute() - 1;
                    }
                    MultiVmSubversion::V2 => {
                        let gas_spent_on_pubdata = tracer
                            .gas_spent_on_pubdata(&self.state.local_state)
                            - spent_pubdata_counter_before;
                        let tx_body_refund = self.tx_body_refund(
                            timestamp_initial,
                            bootloader_refund,
                            gas_spent_on_pubdata,
                        );

                        if tx_body_refund < bootloader_refund {
                            tracing::error!(
                        "Suggested tx body refund is less than bootloader refund. Tx body refund: {}, bootloader refund: {}",
                        tx_body_refund,
                        bootloader_refund
                    );
                        }

                        refund_to_propose = tx_body_refund
                            + self.block_overhead_refund(
                                timestamp_initial,
                                gas_remaining_before,
                                gas_spent_on_pubdata,
                            );

                        let current_tx_index = self.bootloader_state.tx_to_execute() - 1;
                        refund_slot = OPERATOR_REFUNDS_OFFSET + current_tx_index;
                    }
                };

                // Writing the refund into memory
                self.state.memory.memory.write_to_memory(
                    BOOTLOADER_HEAP_PAGE as usize,
                    refund_slot,
                    Some(PrimitiveValue {
                        value: refund_to_propose.into(),
                        is_pointer: false,
                    }),
                    Timestamp(timestamp_before_cycle),
                );
                operator_refund = Some(refund_to_propose);
                tracer.set_refund_as_done();
            }

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
        T: ExecutionEndTracer + PendingRefundTracer + PubdataSpentTracer,
    >(
        &mut self,
        tracer: &mut T,
    ) -> VmExecutionStopReason {
        self.execute_with_custom_tracer_and_refunds(tracer).0
    }

    // Err when transaction is rejected.
    // `Ok(status: TxExecutionStatus::Success)` when the transaction succeeded
    // `Ok(status: TxExecutionStatus::Failure)` when the transaction failed.
    // Note that failed transactions are considered properly processed and are included in blocks
    pub fn execute_next_tx(&mut self) -> Result<VmTxExecutionResult, TxRevertReason> {
        let tx_index = self.bootloader_state.next_unexecuted_tx() as u32;
        let mut tx_tracer = OneTxTracer::default();

        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;

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
                                .get_decommitted_bytes_after_timestamp(timestamp_initial),
                            cycles_used: self.state.local_state.monotonic_cycle_counter
                                - cycles_initial,
                            gas_remaining: self.gas_remaining(),
                        },
                    })
                } else {
                    // VM ended up in state `stop_reason == VmExecutionStopReason::TracerRequestedStop && !tx_tracer.tx_has_been_processed()`.
                    // It means that bootloader successfully finished its execution without executing the transaction.
                    // It is an unexpected situation.
                    panic!("VM successfully finished executing bootloader but transaction wasn't executed");
                }
            }
        }
    }

    /// Returns full VM result and partial result produced within the current execution.
    pub fn execute_till_block_end(&mut self, job_type: BootloaderJobType) -> VmBlockResult {
        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;
        let gas_before = self.gas_remaining();

        let mut tx_result_tracer = TransactionResultTracer::default();
        let stop_reason = self.execute_with_custom_tracer(&mut tx_result_tracer);
        match stop_reason {
            VmExecutionStopReason::VmFinished => {
                let mut full_result = vm_may_have_ended(self, gas_before).unwrap();

                if job_type == BootloaderJobType::TransactionExecution
                    && tx_has_failed(&self.state, 0)
                    && full_result.revert_reason.is_none()
                {
                    let revert_reason = tx_result_tracer
                        .revert_reason
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
                        .get_decommitted_bytes_after_timestamp(timestamp_initial),
                    cycles_used: self.state.local_state.monotonic_cycle_counter - cycles_initial,
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
                VmBlockResult {
                    full_result,
                    block_tip_result,
                }
            }
            VmExecutionStopReason::TracerRequestedStop => {
                unreachable!("NoopMemoryTracer will never stop execution until the block ends")
            }
        }
    }

    /// Unlike `execute_till_block_end` methods returns only result for the block tip execution.
    pub fn execute_block_tip(&mut self) -> VmPartialExecutionResult {
        let timestamp_initial = Timestamp(self.state.local_state.timestamp);
        let cycles_initial = self.state.local_state.monotonic_cycle_counter;
        let mut bootloader_tracer = BootloaderTracer::default();

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
        VmPartialExecutionResult {
            logs: self.collect_execution_logs_after_timestamp(timestamp_initial),
            revert_reason,
            contracts_used: self
                .state
                .decommittment_processor
                .get_decommitted_bytes_after_timestamp(timestamp_initial),
            cycles_used: self.state.local_state.monotonic_cycle_counter - cycles_initial,
            gas_remaining: self.gas_remaining(),
        }
    }

    pub fn execute_validation(
        &mut self,
        validation_params: ValidationTracerParams,
    ) -> Result<(), ValidationError> {
        let mut validation_tracer = ValidationTracer::new(
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
                Err(ValidationError::VioalatedRule(err))
            }
            (VmExecutionStopReason::TracerRequestedStop, None) => Ok(()),
        }
    }

    // returns Some only when there is just one frame in execution trace.
    pub(crate) fn get_final_log_queries(&self) -> Vec<StorageLogQuery> {
        assert_eq!(
            self.state.storage.frames_stack.inner().len(),
            1,
            "VM finished execution in unexpected state"
        );

        let result = self
            .state
            .storage
            .frames_stack
            .inner()
            .current_frame()
            .forward
            .clone();

        result
    }

    fn get_used_contracts(&self) -> Vec<U256> {
        self.state
            .decommittment_processor
            .known_bytecodes
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
            .number_of_updated_storage_slots()
    }
}

// Reads the bootloader memory and checks whether the execution step of the transaction
// has failed.
pub(crate) fn tx_has_failed<S: Storage>(state: &ZkSyncVmState<S>, tx_id: u32) -> bool {
    let mem_slot = RESULT_SUCCESS_FIRST_SLOT + tx_id;
    let mem_value = state
        .memory
        .dump_page_content_as_u256_words(BOOTLOADER_HEAP_PAGE, mem_slot..mem_slot + 1)[0];

    mem_value == U256::zero()
}
