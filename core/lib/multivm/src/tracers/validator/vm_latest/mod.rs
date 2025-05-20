use zk_evm_1_5_2::{
    tracing::{BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{ContextOpcode, FarCallABI, LogOpcode, Opcode, RetOpcode},
};
use zksync_system_constants::KECCAK256_PRECOMPILE_ADDRESS;
use zksync_types::{
    get_code_key, h256_to_address, u256_to_address, u256_to_h256, AccountTreeId, StorageKey, H256,
    U256,
};

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::{TracerExecutionStatus, TracerExecutionStopReason, ViolatedValidationRule},
        Halt,
    },
    tracers::{
        dynamic::vm_1_5_2::DynTracer,
        validator::{
            types::{NewTrustedValidationItems, ValidationTracerMode},
            ValidationRoundResult, ValidationTracer,
        },
    },
    vm_latest::{
        tracers::utils::{computational_gas_price, get_calldata_page_via_abi},
        BootloaderState, SimpleMemory, VmHook, VmTracer, ZkSyncVmState,
    },
    HistoryMode,
};

pub const TIMESTAMP_ASSERTER_FUNCTION_SELECTOR: [u8; 4] = [0x5b, 0x1a, 0x0c, 0x91];

impl<H: HistoryMode> ValidationTracer<H> {
    fn check_user_restrictions_vm_latest<S: WriteStorage>(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H::Vm1_5_2>,
        storage: StoragePtr<S>,
    ) -> ValidationRoundResult {
        if self.computational_gas_used > self.computational_gas_limit {
            return Err(ViolatedValidationRule::TookTooManyComputationalGas(
                self.computational_gas_limit,
            ));
        }

        let opcode_variant = data.opcode.variant;
        match opcode_variant.opcode {
            Opcode::FarCall(_) => {
                let packed_abi = data.src0_value.value;
                let call_destination_value = data.src1_value.value;

                let called_address = u256_to_address(&call_destination_value);
                let far_call_abi = FarCallABI::from_u256(packed_abi);

                if called_address == KECCAK256_PRECOMPILE_ADDRESS
                    && far_call_abi.memory_quasi_fat_pointer.length == 64
                {
                    let calldata_page = get_calldata_page_via_abi(
                        &far_call_abi,
                        state.vm_local_state.callstack.current.base_memory_page,
                    );
                    let calldata = memory.read_unaligned_bytes(
                        calldata_page as usize,
                        far_call_abi.memory_quasi_fat_pointer.start as usize,
                        64,
                    );

                    let slot_to_add =
                        self.slot_to_add_from_keccak_call(&calldata, self.user_address);

                    if let Some(slot) = slot_to_add {
                        return Ok(NewTrustedValidationItems {
                            new_allowed_slots: vec![slot],
                            ..Default::default()
                        });
                    }
                } else if called_address != self.user_address {
                    let code_key = get_code_key(&called_address);
                    let code = storage.borrow_mut().read_value(&code_key);

                    if code == H256::zero() {
                        // The users are not allowed to call contracts with no code
                        return Err(ViolatedValidationRule::CalledContractWithNoCode(
                            called_address,
                        ));
                    }
                    // If this is a call to the timestamp asserter, extract the function arguments and store them in ValidationTraces.
                    // These arguments are used by the mempool for transaction filtering. The call data length should be 68 bytes:
                    // a 4-byte function selector followed by two U256 values.
                    if let Some(params) = &self.timestamp_asserter_params {
                        if called_address == params.address
                            && far_call_abi.memory_quasi_fat_pointer.length == 68
                        {
                            let calldata_page = get_calldata_page_via_abi(
                                &far_call_abi,
                                state.vm_local_state.callstack.current.base_memory_page,
                            );
                            let calldata = memory.read_unaligned_bytes(
                                calldata_page as usize,
                                far_call_abi.memory_quasi_fat_pointer.start as usize,
                                68,
                            );

                            if calldata[..4] == TIMESTAMP_ASSERTER_FUNCTION_SELECTOR {
                                // start and end need to be capped to u64::MAX to avoid overflow
                                let start = U256::from_big_endian(
                                    &calldata[calldata.len() - 64..calldata.len() - 32],
                                )
                                .try_into()
                                .unwrap_or(u64::MAX);
                                let end = U256::from_big_endian(&calldata[calldata.len() - 32..])
                                    .try_into()
                                    .unwrap_or(u64::MAX);

                                // using self.l1_batch_env.timestamp is ok here because the tracer is always
                                // used in a oneshot execution mode
                                if end
                                    < self.l1_batch_timestamp + params.min_time_till_end.as_secs()
                                {
                                    return Err(
                                        ViolatedValidationRule::TimestampAssertionCloseToRangeEnd,
                                    );
                                }

                                self.traces
                                    .lock()
                                    .unwrap()
                                    .apply_timestamp_asserter_range(start..end);
                            }
                        }
                    }
                }
            }
            Opcode::Context(context) => {
                match context {
                    ContextOpcode::Meta => {
                        return Err(ViolatedValidationRule::TouchedDisallowedContext);
                    }
                    ContextOpcode::ErgsLeft => {
                        // TODO (SMA-1168): implement the correct restrictions for the gas left opcode.
                    }
                    _ => {}
                }
            }
            Opcode::Log(LogOpcode::StorageRead) => {
                let key = data.src0_value.value;
                let this_address = state.vm_local_state.callstack.current.this_address;
                let msg_sender = state.vm_local_state.callstack.current.msg_sender;

                if !self.is_allowed_storage_read(storage.clone(), this_address, key, msg_sender) {
                    return Err(ViolatedValidationRule::TouchedDisallowedStorageSlots(
                        this_address,
                        key,
                    ));
                }

                if self.trusted_address_slots.contains(&(this_address, key)) {
                    let storage_key =
                        StorageKey::new(AccountTreeId::new(this_address), u256_to_h256(key));

                    let value = storage.borrow_mut().read_value(&storage_key);

                    return Ok(NewTrustedValidationItems {
                        new_trusted_addresses: vec![h256_to_address(&value)],
                        ..Default::default()
                    });
                }
            }

            Opcode::Ret(RetOpcode::Panic)
                if state.vm_local_state.callstack.current.ergs_remaining == 0 =>
            {
                // Actual gas limit was reached, not the validation gas limit.
                return Err(ViolatedValidationRule::TookTooManyComputationalGas(0));
            }
            _ => {}
        }

        Ok(Default::default())
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H::Vm1_5_2>>
    for ValidationTracer<H>
{
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H::Vm1_5_2>,
        storage: StoragePtr<S>,
    ) {
        // For now, we support only validations for users.
        if let ValidationTracerMode::UserTxValidation = self.validation_mode {
            self.computational_gas_used = self
                .computational_gas_used
                .saturating_add(computational_gas_price(state, &data));

            let validation_round_result =
                self.check_user_restrictions_vm_latest(state, data, memory, storage);
            self.process_validation_round_result(validation_round_result);
        }

        let hook = VmHook::from_opcode_memory(&state, &data, self.vm_version.try_into().unwrap());
        let current_mode = self.validation_mode;
        match (current_mode, hook) {
            (ValidationTracerMode::NoValidation, Some(VmHook::AccountValidationEntered)) => {
                // Account validation can be entered when there is no prior validation (i.e. "nested" validations are not allowed)
                self.validation_mode = ValidationTracerMode::UserTxValidation;
            }
            (ValidationTracerMode::NoValidation, Some(VmHook::PaymasterValidationEntered)) => {
                // Paymaster validation can be entered when there is no prior validation (i.e. "nested" validations are not allowed)
                self.validation_mode = ValidationTracerMode::PaymasterTxValidation;
            }
            (_, Some(VmHook::AccountValidationEntered | VmHook::PaymasterValidationEntered)) => {
                panic!(
                    "Unallowed transition inside the validation tracer. Mode: {:#?}, hook: {:#?}",
                    self.validation_mode, hook
                );
            }
            (_, Some(VmHook::ValidationExited)) => {
                // Validation can be always turned off
                self.validation_mode = ValidationTracerMode::NoValidation;
            }
            (_, Some(VmHook::ValidationStepEnded)) => {
                // The validation step has ended.
                self.should_stop_execution = true;
            }
            (_, _) => {
                // The hook is not relevant to the validation tracer. Ignore.
            }
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H::Vm1_5_2> for ValidationTracer<H> {
    fn finish_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H::Vm1_5_2>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        if self.should_stop_execution {
            return TracerExecutionStatus::Stop(TracerExecutionStopReason::Finish);
        }
        if let Some(result) = self.result.get() {
            return TracerExecutionStatus::Stop(TracerExecutionStopReason::Abort(
                Halt::TracerCustom(format!("Validation error: {:#?}", result)),
            ));
        }
        TracerExecutionStatus::Continue
    }
}
