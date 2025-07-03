use std::{collections::HashMap, fmt, mem, rc::Rc};

use zk_evm_1_5_0::{
    aux_structures::LogQuery, zkevm_opcode_defs::system_params::INITIAL_FRAME_FORMAL_EH_LOCATION,
};
use zksync_types::{
    bytecode::BytecodeHash, h256_to_u256, l1::is_l1_tx_type, l2_to_l1_log::UserL2ToL1Log,
    u256_to_h256, writes::StateDiffRecord, AccountTreeId, StorageKey, StorageLog, StorageLogKind,
    StorageLogWithPreviousValue, Transaction, BOOTLOADER_ADDRESS, H160, H256,
    KNOWN_CODES_STORAGE_ADDRESS, L1_MESSENGER_ADDRESS, U256,
};
use zksync_vm2::{
    interface::{CallframeInterface, HeapId, StateInterface, Tracer},
    ExecutionEnd, FatPointer, Settings, VirtualMachine,
};

use super::{
    bytecode::compress_bytecodes,
    tracers::{ValidationTracer, WithBuiltinTracers},
    world::World,
};
use crate::{
    glue::GlueInto,
    interface::{
        pubdata::{PubdataBuilder, PubdataInput},
        storage::{ImmutableStorageView, ReadStorage, StoragePtr, StorageView},
        BytecodeCompressionError, BytecodeCompressionResult, CurrentExecutionState,
        ExecutionResult, FinishedL1Batch, Halt, InspectExecutionMode, L1BatchEnv, L2BlockEnv,
        PushTransactionResult, Refunds, SystemEnv, TxRevertReason, VmEvent, VmExecutionLogs,
        VmExecutionMode, VmExecutionResultAndLogs, VmExecutionStatistics, VmFactory, VmInterface,
        VmInterfaceHistoryEnabled, VmRevertReason, VmTrackingContracts,
    },
    utils::events::extract_l2tol1logs_from_l1_messenger,
    vm_fast::{events::merge_events, version::FastVmVersion, FastValidationTracer},
    vm_latest::{
        bootloader::{
            utils::{apply_l2_block, apply_pubdata_to_memory},
            BootloaderState, BootloaderStateSnapshot,
        },
        constants::{
            get_operator_refunds_offset, get_result_success_first_slot,
            get_vm_hook_params_start_position, get_vm_hook_position, TX_GAS_LIMIT_OFFSET,
            VM_HOOK_PARAMS_COUNT,
        },
        utils::refund::compute_refund,
        TransactionData, VmHook,
    },
    VmVersion,
};

#[derive(Debug)]
struct VmRunResult {
    execution_result: ExecutionResult,
    /// `true` if VM execution has terminated (as opposed to being stopped on a hook, e.g. when executing a single transaction
    /// in a batch). Used for `execution_result == Revert { .. }` to understand whether VM logs should be reverted.
    execution_ended: bool,
    refunds: Refunds,
    /// This value is used in stats. It's defined in the old VM as the latest value used when computing refunds (see the refunds tracer for `vm_latest`).
    /// This is **not** equal to the pubdata diff before and after VM execution; e.g., when executing a batch tip,
    /// `pubdata_published` is always 0 (since no refunds are computed).
    pubdata_published: u32,
}

impl VmRunResult {
    fn should_ignore_vm_logs(&self) -> bool {
        match &self.execution_result {
            ExecutionResult::Success { .. } => false,
            ExecutionResult::Halt { .. } => true,
            // Logs generated during reverts should only be ignored if the revert has reached the root (bootloader) call frame,
            // which is only possible with `TxExecutionMode::EthCall`.
            ExecutionResult::Revert { .. } => self.execution_ended,
        }
    }
}

type InnerVm<S, Tr, Val> =
    VirtualMachine<WithBuiltinTracers<Tr, Val>, World<S, WithBuiltinTracers<Tr, Val>>>;

/// Fast VM wrapper.
///
/// The wrapper is parametric by the storage and tracer types. Besides the [`Tracer`] trait, the tracer must implement [`Default`]
/// (the latter is necessary to complete batches). Validation is encapsulated in a separate type param. It should be set to `()`
/// for "standard" validation (not stopping after validation; no validation-specific checks), or [`FullValidationTracer`](super::FullValidationTracer)
/// for full validation (stopping after validation; validation-specific checks).
pub struct Vm<S, Tr = (), Val = FastValidationTracer> {
    pub(super) world: World<S, WithBuiltinTracers<Tr, Val>>,
    pub(super) inner: InnerVm<S, Tr, Val>,
    pub(super) bootloader_state: BootloaderState,
    pub(super) batch_env: L1BatchEnv,
    pub(super) system_env: SystemEnv,
    snapshot: Option<VmSnapshot>,
    vm_version: FastVmVersion,
    skip_signature_verification: bool,
    #[cfg(test)]
    enforced_state_diffs: Option<Vec<StateDiffRecord>>,
}

impl<S: ReadStorage, Tr: Tracer, Val: ValidationTracer> Vm<S, Tr, Val> {
    pub fn custom(batch_env: L1BatchEnv, system_env: SystemEnv, storage: S) -> Self {
        let vm_version: FastVmVersion = VmVersion::from(system_env.version)
            .try_into()
            .unwrap_or_else(|_| {
                panic!(
                    "Protocol version {:?} is not supported by fast VM",
                    system_env.version
                )
            });

        let default_aa_code_hash = system_env.base_system_smart_contracts.default_aa.hash;
        let evm_emulator_hash = system_env
            .base_system_smart_contracts
            .evm_emulator
            .as_ref()
            .map(|evm| evm.hash)
            .unwrap_or(system_env.base_system_smart_contracts.default_aa.hash);

        let mut program_cache = HashMap::from([World::convert_system_contract_code(
            &system_env.base_system_smart_contracts.default_aa,
            false,
        )]);
        if let Some(evm_emulator) = &system_env.base_system_smart_contracts.evm_emulator {
            let (bytecode_hash, program) = World::convert_system_contract_code(evm_emulator, false);
            program_cache.insert(bytecode_hash, program);
        }

        let (_, bootloader) = World::convert_system_contract_code(
            &system_env.base_system_smart_contracts.bootloader,
            true,
        );
        let bootloader_memory = BootloaderState::initial_memory(&batch_env);

        let mut inner = VirtualMachine::new(
            BOOTLOADER_ADDRESS,
            bootloader,
            H160::zero(),
            &[],
            system_env.bootloader_gas_limit,
            Settings {
                default_aa_code_hash: default_aa_code_hash.into(),
                evm_interpreter_code_hash: evm_emulator_hash.into(),
                hook_address: get_vm_hook_position(vm_version.into()) * 32,
            },
        );

        inner.current_frame().set_stack_pointer(0);
        // The bootloader writes results to high addresses in its heap, so it makes sense to preallocate it.
        inner.current_frame().set_heap_bound(u32::MAX);
        inner.current_frame().set_aux_heap_bound(u32::MAX);
        inner
            .current_frame()
            .set_exception_handler(INITIAL_FRAME_FORMAL_EH_LOCATION);

        let mut this = Self {
            world: World::new(storage, program_cache),
            inner,
            bootloader_state: BootloaderState::new(
                system_env.execution_mode,
                bootloader_memory.clone(),
                batch_env.first_l2_block.clone(),
                system_env.version,
            ),
            system_env,
            batch_env,
            snapshot: None,
            vm_version,
            skip_signature_verification: false,
            #[cfg(test)]
            enforced_state_diffs: None,
        };
        this.write_to_bootloader_heap(bootloader_memory);
        this
    }

    pub fn skip_signature_verification(&mut self) {
        self.skip_signature_verification = true;
    }

    fn get_hook_params(&self) -> [U256; 3] {
        (get_vm_hook_params_start_position(self.vm_version.into())
            ..get_vm_hook_params_start_position(self.vm_version.into()) + VM_HOOK_PARAMS_COUNT)
            .map(|word| self.read_word_from_bootloader_heap(word as usize))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    fn get_tx_result(&self) -> U256 {
        let tx_idx = self.bootloader_state.current_tx();
        let slot = get_result_success_first_slot(self.vm_version.into()) as usize + tx_idx;
        self.read_word_from_bootloader_heap(slot)
    }

    fn get_debug_log(&self) -> (String, String) {
        let hook_params = self.get_hook_params();
        let mut msg = u256_to_h256(hook_params[0]).as_bytes().to_vec();
        // Trim 0 byte padding at the end.
        while msg.last() == Some(&0) {
            msg.pop();
        }

        let data = hook_params[1];
        let msg = String::from_utf8(msg).expect("Invalid debug message");

        // For long data, it is better to use hex-encoding for greater readability
        let data_str = if data > U256::from(u64::MAX) {
            format!("0x{data:x}")
        } else {
            data.to_string()
        };
        (msg, data_str)
    }

    /// Should only be used when the bootloader is executing (e.g., when handling hooks).
    pub(crate) fn read_word_from_bootloader_heap(&self, word: usize) -> U256 {
        let start_address = word as u32 * 32;
        self.inner.read_heap_u256(HeapId::FIRST, start_address)
    }

    fn read_bytes_from_heap(&self, ptr: FatPointer) -> Vec<u8> {
        assert_eq!(ptr.offset, 0);
        (ptr.start..ptr.start + ptr.length)
            .map(|addr| self.inner.read_heap_byte(ptr.memory_page, addr))
            .collect()
    }

    pub(crate) fn has_previous_far_calls(&mut self) -> bool {
        let callframe_count = self.inner.number_of_callframes();
        (1..callframe_count).any(|i| !self.inner.callframe(i).is_near_call())
    }

    /// Should only be used when the bootloader is executing (e.g., when handling hooks).
    pub(crate) fn write_to_bootloader_heap(
        &mut self,
        memory: impl IntoIterator<Item = (usize, U256)>,
    ) {
        assert!(
            !self.has_previous_far_calls(),
            "Cannot write to bootloader heap when not in root call frame"
        );

        for (slot, value) in memory {
            let start_address = slot as u32 * 32;
            self.inner
                .write_heap_u256(HeapId::FIRST, start_address, value);
        }
    }

    pub(crate) fn insert_bytecodes<'a>(&mut self, bytecodes: impl IntoIterator<Item = &'a [u8]>) {
        for code in bytecodes {
            let hash = BytecodeHash::for_bytecode(code).value_u256();
            self.world.bytecode_cache.insert(hash, code.into());
        }
    }

    pub(crate) fn push_transaction_inner(
        &mut self,
        tx: Transaction,
        refund: u64,
        with_compression: bool,
    ) {
        let use_evm_emulator = self
            .system_env
            .base_system_smart_contracts
            .evm_emulator
            .is_some();
        let tx = TransactionData::new(tx, use_evm_emulator);
        let overhead = tx.overhead_gas();

        self.insert_bytecodes(tx.factory_deps.iter().map(|dep| &dep[..]));

        let compressed_bytecodes = if is_l1_tx_type(tx.tx_type) || !with_compression {
            // L1 transactions do not need compression
            vec![]
        } else {
            compress_bytecodes(&tx.factory_deps, |hash| {
                self.inner
                    .world_diff()
                    .get_storage_state()
                    .get(&(KNOWN_CODES_STORAGE_ADDRESS, h256_to_u256(hash)))
                    .map(|x| !x.is_zero())
                    .unwrap_or_else(|| self.world.storage.is_bytecode_known(&hash))
            })
        };

        let trusted_ergs_limit = tx.trusted_ergs_limit();
        let tx_origin = tx.from;
        let (memory, ecrecover_call) = self.bootloader_state.push_tx(
            tx,
            overhead,
            refund,
            compressed_bytecodes,
            trusted_ergs_limit,
            self.system_env.chain_id,
        );
        self.write_to_bootloader_heap(memory);

        // The expected `ecrecover` call params *must* be reset on each transaction.
        // We only set call params for transactions using the default AA. Other AAs may expect another
        // address returned from `ecrecover`, or may not invoke `ecrecover` at all during validation
        // (both of which would make caching a security hazard).
        let needs_default_aa_check = self.skip_signature_verification && ecrecover_call.is_some();
        self.world.precompiles.expected_ecrecover_call =
            if needs_default_aa_check && self.world.has_default_aa(&tx_origin) {
                ecrecover_call
            } else {
                None
            };
    }

    #[cfg(test)]
    pub(super) fn enforce_state_diffs(&mut self, diffs: Vec<StateDiffRecord>) {
        self.enforced_state_diffs = Some(diffs);
    }

    fn compute_state_diffs(&mut self) -> Vec<StateDiffRecord> {
        #[cfg(test)]
        if let Some(enforced_diffs) = self.enforced_state_diffs.take() {
            return enforced_diffs;
        }

        let storage = &mut self.world.storage;
        let diffs =
            self.inner
                .world_diff()
                .get_storage_changes()
                .map(move |((address, key), change)| {
                    let storage_key =
                        StorageKey::new(AccountTreeId::new(address), u256_to_h256(key));
                    StateDiffRecord {
                        address,
                        key,
                        derived_key: LogQuery::derive_final_address_for_params(&address, &key),
                        enumeration_index: storage
                            .get_enumeration_index(&storage_key)
                            .unwrap_or_default(),
                        initial_value: change.before,
                        final_value: change.after,
                    }
                });
        diffs
            .filter(|diff| diff.address != L1_MESSENGER_ADDRESS)
            .collect()
    }

    pub(crate) fn decommitted_hashes(&self) -> impl Iterator<Item = U256> + '_ {
        self.inner.world_diff().decommitted_hashes()
    }

    pub(super) fn gas_remaining(&mut self) -> u32 {
        self.inner.current_frame().gas()
    }

    // visible for testing
    pub(super) fn get_current_execution_state(&self) -> CurrentExecutionState {
        let world_diff = self.inner.world_diff();
        let vm = &self.inner;
        let events = merge_events(vm.events(), self.batch_env.number);

        let user_l2_to_l1_logs = extract_l2tol1logs_from_l1_messenger(&events)
            .into_iter()
            .map(Into::into)
            .map(UserL2ToL1Log)
            .collect();

        CurrentExecutionState {
            events,
            deduplicated_storage_logs: world_diff
                .get_storage_changes()
                .map(|((address, key), change)| StorageLog {
                    key: StorageKey::new(AccountTreeId::new(address), u256_to_h256(key)),
                    value: u256_to_h256(change.after),
                    kind: StorageLogKind::RepeatedWrite, // Initialness doesn't matter here
                })
                .collect(),
            used_contract_hashes: self.decommitted_hashes().collect(),
            system_logs: vm.l2_to_l1_logs().map(GlueInto::glue_into).collect(),
            user_l2_to_l1_logs,
            storage_refunds: world_diff.storage_refunds().to_vec(),
            pubdata_costs: world_diff.pubdata_costs().to_vec(),
        }
    }
}

#[derive(Debug)]
struct AccountValidationGasSplit {
    gas_given: u32,
    gas_hidden: u32,
}

impl<S, Tr, Val> Vm<S, Tr, Val>
where
    S: ReadStorage,
    Tr: Tracer + Default,
    Val: ValidationTracer,
{
    fn run(
        &mut self,
        execution_mode: VmExecutionMode,
        tracer: &mut WithBuiltinTracers<Tr, Val>,
        track_refunds: bool,
        pubdata_builder: Option<&dyn PubdataBuilder>,
    ) -> VmRunResult {
        let mut gas_left_for_account_validation =
            self.system_env.default_validation_computational_gas_limit;
        let mut account_validation_gas_split = None;

        let mut refunds = Refunds {
            gas_refunded: 0,
            operator_suggested_refund: 0,
        };
        let mut last_tx_result = None;
        let mut pubdata_before = self.inner.pubdata() as u32;
        let mut pubdata_published = 0;

        let (execution_result, execution_ended) = loop {
            let hook = match self.inner.run(&mut self.world, tracer) {
                ExecutionEnd::SuspendedOnHook(hook) => hook,
                ExecutionEnd::ProgramFinished(output) => {
                    break (ExecutionResult::Success { output }, true);
                }
                ExecutionEnd::Reverted(output) => {
                    let result = match TxRevertReason::parse_error(&output) {
                        TxRevertReason::TxReverted(output) => ExecutionResult::Revert { output },
                        TxRevertReason::Halt(reason) => ExecutionResult::Halt { reason },
                    };
                    break (result, true);
                }
                ExecutionEnd::Panicked => {
                    let reason = if self.gas_remaining() == 0 {
                        Halt::BootloaderOutOfGas
                    } else {
                        Halt::VMPanic
                    };
                    break (ExecutionResult::Halt { reason }, true);
                }
                ExecutionEnd::StoppedByTracer => {
                    break (
                        ExecutionResult::Halt {
                            reason: Halt::TracerCustom(
                                "Unexpectedly stopped by tracer".to_string(),
                            ),
                        },
                        false,
                    );
                }
            };

            let hook = VmHook::new(hook);
            match hook {
                VmHook::AccountValidationEntered => {
                    assert!(
                        account_validation_gas_split.is_none(),
                        "Account validation can't be nested"
                    );
                    let gas = self.gas_remaining();
                    let gas_given = gas.min(gas_left_for_account_validation);
                    let gas_hidden = gas - gas_given;
                    tracer
                        .validation
                        .account_validation_entered(gas_given, gas_hidden);

                    account_validation_gas_split = Some(AccountValidationGasSplit {
                        gas_given,
                        gas_hidden,
                    });
                    // As long as gasleft is allowed during account validation,
                    // the VM must not be used in the sequencer because a malicious
                    // account cause proving failure by checking if gasleft > 100k
                    self.inner.current_frame().set_gas(gas_given);
                }

                VmHook::ValidationExited => {
                    if let Some(AccountValidationGasSplit {
                        gas_given,
                        gas_hidden,
                    }) = account_validation_gas_split.take()
                    {
                        let gas_left = self.inner.current_frame().gas();
                        gas_left_for_account_validation -= gas_given - gas_left;
                        self.inner.current_frame().set_gas(gas_left + gas_hidden);
                    }

                    if let Some(reason) = tracer.validation.validation_exited() {
                        break (ExecutionResult::Halt { reason }, true);
                    }
                }

                VmHook::ValidationStepEnded => {
                    if Val::STOP_AFTER_VALIDATION {
                        break (ExecutionResult::Success { output: vec![] }, true);
                    }
                }

                VmHook::TxHasEnded => {
                    if let VmExecutionMode::OneTx = execution_mode {
                        // The bootloader may invoke `TxHasEnded` hook without posting a tx result previously. One case when this can happen
                        // is estimating gas for L1 transactions, if a transaction runs out of gas during execution.
                        let tx_result = last_tx_result.take().unwrap_or_else(|| {
                            let tx_has_failed = self.get_tx_result().is_zero();
                            if tx_has_failed {
                                let output = VmRevertReason::General {
                                    msg: "Transaction reverted with empty reason. Possibly out of gas"
                                        .to_string(),
                                    data: vec![],
                                };
                                ExecutionResult::Revert { output }
                            } else {
                                ExecutionResult::Success { output: vec![] }
                            }
                        });
                        break (tx_result, false);
                    }
                }
                VmHook::AskOperatorForRefund => {
                    if track_refunds {
                        let [bootloader_refund, gas_spent_on_pubdata, gas_per_pubdata_byte] =
                            self.get_hook_params();
                        let current_tx_index = self.bootloader_state.current_tx();
                        let tx_description_offset = self
                            .bootloader_state
                            .get_tx_description_offset(current_tx_index);
                        let tx_gas_limit = self
                            .read_word_from_bootloader_heap(
                                tx_description_offset + TX_GAS_LIMIT_OFFSET,
                            )
                            .as_u64();

                        let pubdata_after = self.inner.pubdata() as u32;
                        pubdata_published = pubdata_after.saturating_sub(pubdata_before);

                        refunds.operator_suggested_refund = compute_refund(
                            &self.batch_env,
                            bootloader_refund.as_u64(),
                            gas_spent_on_pubdata.as_u64(),
                            tx_gas_limit,
                            gas_per_pubdata_byte.low_u32(),
                            pubdata_published,
                            self.bootloader_state
                                .last_l2_block()
                                .txs
                                .last()
                                .unwrap()
                                .hash,
                        );

                        pubdata_before = pubdata_after;
                        let refund_value = refunds.operator_suggested_refund;
                        self.write_to_bootloader_heap([(
                            get_operator_refunds_offset(self.vm_version.into()) + current_tx_index,
                            refund_value.into(),
                        )]);
                        self.bootloader_state
                            .set_refund_for_current_tx(refund_value);
                    }
                }
                VmHook::NotifyAboutRefund => {
                    if track_refunds {
                        refunds.gas_refunded = self.get_hook_params()[0].low_u64()
                    }
                }
                VmHook::PostResult => {
                    let result = self.get_hook_params()[0];
                    let value = self.get_hook_params()[1];
                    let fp = FatPointer::from(value);
                    let return_data = self.read_bytes_from_heap(fp);

                    last_tx_result = Some(if result.is_zero() {
                        ExecutionResult::Revert {
                            output: VmRevertReason::from(return_data.as_slice()),
                        }
                    } else {
                        ExecutionResult::Success {
                            output: return_data,
                        }
                    });
                }
                VmHook::FinalBatchInfo => {
                    // set fictive l2 block
                    let interop_root_application_config =
                        self.bootloader_state.get_interop_root_application_config();
                    let txs_index = self.bootloader_state.free_tx_index();
                    let l2_block = self.bootloader_state.insert_fictive_l2_block();
                    let mut memory = vec![];
                    apply_l2_block(
                        &mut memory,
                        l2_block,
                        txs_index,
                        self.vm_version.into(),
                        Some(interop_root_application_config),
                    );
                    self.write_to_bootloader_heap(memory);
                }
                VmHook::PubdataRequested => {
                    if !matches!(execution_mode, VmExecutionMode::Batch) {
                        unreachable!("We do not provide the pubdata when executing the block tip or a single transaction");
                    }

                    let events = merge_events(self.inner.events(), self.batch_env.number);

                    let published_bytecodes = events
                        .iter()
                        .filter(|event| {
                            // Filter events from the l1 messenger contract that match the expected signature.
                            event.address == L1_MESSENGER_ADDRESS
                                && !event.indexed_topics.is_empty()
                                && event.indexed_topics[0]
                                    == VmEvent::L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE
                        })
                        .map(|event| {
                            let hash = U256::from_big_endian(&event.value[..32]);
                            self.world
                                .bytecode_cache
                                .get(&hash)
                                .expect("published unknown bytecode")
                                .clone()
                        })
                        .collect();

                    let pubdata_input = PubdataInput {
                        user_logs: extract_l2tol1logs_from_l1_messenger(&events),
                        l2_to_l1_messages: VmEvent::extract_long_l2_to_l1_messages(&events),
                        published_bytecodes,
                        state_diffs: self.compute_state_diffs(),
                    };

                    // Save the pubdata for the future initial bootloader memory building
                    self.bootloader_state
                        .set_pubdata_input(pubdata_input.clone());

                    // Apply the pubdata to the current memory
                    let mut memory_to_apply = vec![];

                    apply_pubdata_to_memory(
                        &mut memory_to_apply,
                        pubdata_builder.expect("`pubdata_builder` is required to finish batch"),
                        &pubdata_input,
                        self.system_env.version,
                        self.vm_version.into(),
                    );
                    self.write_to_bootloader_heap(memory_to_apply);
                }

                VmHook::PaymasterValidationEntered => { /* unused */ }
                VmHook::DebugLog => {
                    let (log, log_arg) = self.get_debug_log();
                    let last_tx = self.bootloader_state.last_l2_block().txs.last();
                    let tx_hash = last_tx.map(|tx| tx.hash);
                    tracing::trace!(tx = ?tx_hash, "{log}: {log_arg}");
                }
                VmHook::DebugReturnData | VmHook::NearCallCatch => {
                    // These hooks are for debug purposes only
                }
            }
        };

        VmRunResult {
            execution_result,
            execution_ended,
            refunds,
            pubdata_published,
        }
    }

    pub(crate) fn inspect_inner(
        &mut self,
        tracer: &mut (Tr, Val),
        execution_mode: VmExecutionMode,
        pubdata_builder: Option<&dyn PubdataBuilder>,
    ) -> VmExecutionResultAndLogs {
        let mut track_refunds = false;
        if matches!(execution_mode, VmExecutionMode::OneTx) {
            // Move the pointer to the next transaction
            self.bootloader_state.move_tx_to_execute_pointer();
            track_refunds = true;
        }

        let start = self.inner.world_diff().snapshot();
        let gas_before = self.gas_remaining();
        let (external, validation) = mem::take(tracer);
        let mut full_tracer =
            WithBuiltinTracers::new(external, validation, self.world.dynamic_bytecodes.clone());

        let result = self.run(
            execution_mode,
            &mut full_tracer,
            track_refunds,
            pubdata_builder,
        );
        let circuit_statistic = full_tracer.circuit_statistic();
        *tracer = (full_tracer.external, full_tracer.validation);

        let ignore_world_diff =
            matches!(execution_mode, VmExecutionMode::OneTx) && result.should_ignore_vm_logs();

        // If the execution is halted, the VM changes are expected to be rolled back by the caller.
        // Earlier VMs return empty execution logs in this case, so we follow this behavior.
        // Likewise, if a revert has reached the bootloader frame (possible with `TxExecutionMode::EthCall`; otherwise, the bootloader catches reverts),
        // old VMs revert all logs; the new VM doesn't do that automatically, so we recreate this behavior here.
        let logs = if ignore_world_diff {
            VmExecutionLogs::default()
        } else {
            let storage_logs = self
                .inner
                .world_diff()
                .get_storage_changes_after(&start)
                .map(|((address, key), change)| StorageLogWithPreviousValue {
                    log: StorageLog {
                        key: StorageKey::new(AccountTreeId::new(address), u256_to_h256(key)),
                        value: u256_to_h256(change.after),
                        kind: if change.is_initial {
                            StorageLogKind::InitialWrite
                        } else {
                            StorageLogKind::RepeatedWrite
                        },
                    },
                    previous_value: u256_to_h256(change.before),
                })
                .collect();
            let events = merge_events(
                self.inner.world_diff().events_after(&start).iter().copied(),
                self.batch_env.number,
            );
            let user_l2_to_l1_logs = extract_l2tol1logs_from_l1_messenger(&events)
                .into_iter()
                .map(Into::into)
                .map(UserL2ToL1Log)
                .collect();
            let system_l2_to_l1_logs = self
                .inner
                .world_diff()
                .l2_to_l1_logs_after(&start)
                .iter()
                .map(|&log| log.glue_into())
                .collect();
            VmExecutionLogs {
                storage_logs,
                events,
                user_l2_to_l1_logs,
                system_l2_to_l1_logs,
                total_log_queries_count: 0, // This field is unused
            }
        };

        let gas_remaining = self.gas_remaining();
        let gas_used = gas_before - gas_remaining;

        // We need to filter out bytecodes the deployment of which may have been reverted; the tracer is not aware of reverts.
        // To do this, we check bytecodes against deployer events.
        let factory_deps_marked_as_known = VmEvent::extract_bytecodes_marked_as_known(&logs.events);
        let dynamic_factory_deps = self
            .world
            .decommit_dynamic_bytecodes(factory_deps_marked_as_known);

        VmExecutionResultAndLogs {
            result: result.execution_result,
            logs,
            // TODO (PLA-936): Fill statistics; investigate whether they should be zeroed on `Halt`
            statistics: VmExecutionStatistics {
                gas_used: gas_used.into(),
                gas_remaining,
                computational_gas_used: gas_used, // since 1.5.0, this always has the same value as `gas_used`
                pubdata_published: result.pubdata_published,
                circuit_statistic,
                contracts_used: 0,
                cycles_used: 0,
                total_log_queries: 0,
            },
            refunds: result.refunds,
            dynamic_factory_deps,
        }
    }
}

impl<S, Tr, Val> VmFactory<StorageView<S>> for Vm<ImmutableStorageView<S>, Tr, Val>
where
    S: ReadStorage,
    Tr: Tracer + Default,
    Val: ValidationTracer,
{
    fn new(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Self {
        let storage = ImmutableStorageView::new(storage);
        Self::custom(batch_env, system_env, storage)
    }
}

impl<S, Tr, Val> VmInterface for Vm<S, Tr, Val>
where
    S: ReadStorage,
    Tr: Tracer + Default,
    Val: ValidationTracer,
{
    type TracerDispatcher = (Tr, Val);

    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult<'_> {
        self.push_transaction_inner(tx, 0, true);
        PushTransactionResult {
            compressed_bytecodes: self
                .bootloader_state
                .get_last_tx_compressed_bytecodes()
                .into(),
        }
    }

    fn inspect(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.inspect_inner(tracer, execution_mode.into(), None)
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        self.push_transaction_inner(tx, 0, with_compression);
        let result = self.inspect(tracer, InspectExecutionMode::OneTx);

        let compression_result = if self.has_unpublished_bytecodes() {
            Err(BytecodeCompressionError::BytecodeCompressionFailed)
        } else {
            Ok(self
                .bootloader_state
                .get_last_tx_compressed_bytecodes()
                .into())
        };
        (compression_result, result)
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.bootloader_state.start_new_l2_block(l2_block_env)
    }

    fn finish_batch(&mut self, pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        let result = self.inspect_inner(
            &mut Default::default(),
            VmExecutionMode::Batch,
            Some(pubdata_builder.as_ref()),
        );
        let execution_state = self.get_current_execution_state();
        let bootloader_memory = self
            .bootloader_state
            .bootloader_memory(pubdata_builder.as_ref());
        FinishedL1Batch {
            block_tip_execution_result: result,
            final_execution_state: execution_state,
            final_bootloader_memory: Some(bootloader_memory),
            pubdata_input: Some(
                self.bootloader_state
                    .settlement_layer_pubdata(pubdata_builder.as_ref()),
            ),
            state_diffs: Some(
                self.bootloader_state
                    .get_pubdata_information()
                    .state_diffs
                    .to_vec(),
            ),
        }
    }
}

#[derive(Debug)]
struct VmSnapshot {
    bootloader_snapshot: BootloaderStateSnapshot,
}

impl<S, Tr, Val> VmInterfaceHistoryEnabled for Vm<S, Tr, Val>
where
    S: ReadStorage,
    Tr: Tracer + Default,
    Val: ValidationTracer,
{
    fn make_snapshot(&mut self) {
        assert!(
            self.snapshot.is_none(),
            "cannot create a VM snapshot until a previous snapshot is rolled back to or popped"
        );

        self.inner.make_snapshot();
        self.snapshot = Some(VmSnapshot {
            bootloader_snapshot: self.bootloader_state.get_snapshot(),
        });
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        let VmSnapshot {
            bootloader_snapshot,
        } = self.snapshot.take().expect("no snapshots to rollback to");

        self.inner.rollback();
        self.bootloader_state.apply_snapshot(bootloader_snapshot);
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.inner.pop_snapshot();
        self.snapshot = None;
    }
}

impl<S: ReadStorage, Tr: Tracer> VmTrackingContracts for Vm<S, Tr>
where
    Self: VmInterface,
{
    fn used_contract_hashes(&self) -> Vec<H256> {
        self.decommitted_hashes().map(u256_to_h256).collect()
    }
}

impl<S: fmt::Debug, Tr: fmt::Debug, Val: fmt::Debug> fmt::Debug for Vm<S, Tr, Val> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vm")
            .field("bootloader_state", &self.bootloader_state)
            .field("world", &self.world)
            .field("batch_env", &self.batch_env)
            .field("system_env", &self.system_env)
            .field("snapshot", &self.snapshot.as_ref().map(|_| ()))
            .finish()
    }
}
