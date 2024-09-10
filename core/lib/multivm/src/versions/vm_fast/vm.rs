use std::{collections::HashMap, fmt};

use vm2::{
    decode::decode_program, CallframeInterface, ExecutionEnd, FatPointer, HeapId, Program,
    Settings, StateInterface, Tracer, VirtualMachine,
};
use zk_evm_1_5_0::zkevm_opcode_defs::system_params::INITIAL_FRAME_FORMAL_EH_LOCATION;
use zksync_contracts::SystemContractCode;
use zksync_types::{
    l1::is_l1_tx_type,
    l2_to_l1_log::UserL2ToL1Log,
    utils::key_for_eth_balance,
    writes::{
        compression::compress_with_best_strategy, StateDiffRecord, BYTES_PER_DERIVED_KEY,
        BYTES_PER_ENUMERATION_INDEX,
    },
    AccountTreeId, StorageKey, StorageLog, StorageLogKind, StorageLogWithPreviousValue,
    BOOTLOADER_ADDRESS, H160, KNOWN_CODES_STORAGE_ADDRESS, L1_MESSENGER_ADDRESS,
    L2_BASE_TOKEN_ADDRESS, U256,
};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256, u256_to_h256};

use super::{
    bootloader_state::{BootloaderState, BootloaderStateSnapshot},
    bytecode::compress_bytecodes,
    circuits_tracer::CircuitsTracer,
    hook::Hook,
    initial_bootloader_memory::bootloader_initial_memory,
    transaction_data::TransactionData,
};
use crate::{
    glue::GlueInto,
    interface::{
        storage::ReadStorage, BytecodeCompressionError, BytecodeCompressionResult,
        CurrentExecutionState, ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv, L2BlockEnv,
        Refunds, SystemEnv, TxRevertReason, VmEvent, VmExecutionLogs, VmExecutionMode,
        VmExecutionResultAndLogs, VmExecutionStatistics, VmInterface, VmInterfaceHistoryEnabled,
        VmMemoryMetrics, VmRevertReason,
    },
    utils::events::extract_l2tol1logs_from_l1_messenger,
    vm_fast::{
        bootloader_state::utils::{apply_l2_block, apply_pubdata_to_memory},
        events::merge_events,
        pubdata::PubdataInput,
        refund::compute_refund,
    },
    vm_latest::{
        constants::{
            get_vm_hook_params_start_position, get_vm_hook_position, OPERATOR_REFUNDS_OFFSET,
            TX_GAS_LIMIT_OFFSET, VM_HOOK_PARAMS_COUNT,
        },
        MultiVMSubversion,
    },
};

const VM_VERSION: MultiVMSubversion = MultiVMSubversion::IncreasedBootloaderMemory;

pub struct Vm<S> {
    pub(crate) world: World<S, CircuitsTracer>,
    pub(crate) inner: VirtualMachine<CircuitsTracer, World<S, CircuitsTracer>>,
    gas_for_account_validation: u32,
    pub(crate) bootloader_state: BootloaderState,
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,
    snapshot: Option<VmSnapshot>,
    #[cfg(test)]
    enforced_state_diffs: Option<Vec<StateDiffRecord>>,
}

impl<S: ReadStorage> Vm<S> {
    fn run(
        &mut self,
        execution_mode: VmExecutionMode,
        tracer: &mut CircuitsTracer,
        track_refunds: bool,
    ) -> (ExecutionResult, Refunds) {
        let mut refunds = Refunds {
            gas_refunded: 0,
            operator_suggested_refund: 0,
        };
        let mut last_tx_result = None;
        let mut pubdata_before = self.inner.world_diff().pubdata() as u32;

        let result = loop {
            let hook = match self.inner.run(&mut self.world, tracer) {
                ExecutionEnd::SuspendedOnHook(hook) => hook,
                ExecutionEnd::ProgramFinished(output) => break ExecutionResult::Success { output },
                ExecutionEnd::Reverted(output) => {
                    break match TxRevertReason::parse_error(&output) {
                        TxRevertReason::TxReverted(output) => ExecutionResult::Revert { output },
                        TxRevertReason::Halt(reason) => ExecutionResult::Halt { reason },
                    }
                }
                ExecutionEnd::Panicked => {
                    break ExecutionResult::Halt {
                        reason: if self.gas_remaining() == 0 {
                            Halt::BootloaderOutOfGas
                        } else {
                            Halt::VMPanic
                        },
                    }
                }
            };

            match Hook::from_u32(hook) {
                Hook::AccountValidationEntered | Hook::AccountValidationExited => {
                    // TODO (PLA-908): implement account validation
                }
                Hook::TxHasEnded => {
                    if let VmExecutionMode::OneTx = execution_mode {
                        break last_tx_result.take().unwrap();
                    }
                }
                Hook::AskOperatorForRefund => {
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

                        let pubdata_published = self.inner.world_diff().pubdata() as u32;

                        refunds.operator_suggested_refund = compute_refund(
                            &self.batch_env,
                            bootloader_refund.as_u64(),
                            gas_spent_on_pubdata.as_u64(),
                            tx_gas_limit,
                            gas_per_pubdata_byte.low_u32(),
                            pubdata_published.saturating_sub(pubdata_before),
                            self.bootloader_state
                                .last_l2_block()
                                .txs
                                .last()
                                .unwrap()
                                .hash,
                        );

                        pubdata_before = pubdata_published;
                        let refund_value = refunds.operator_suggested_refund;
                        self.write_to_bootloader_heap([(
                            OPERATOR_REFUNDS_OFFSET + current_tx_index,
                            refund_value.into(),
                        )]);
                        self.bootloader_state
                            .set_refund_for_current_tx(refund_value);
                    }
                }
                Hook::NotifyAboutRefund => {
                    if track_refunds {
                        refunds.gas_refunded = self.get_hook_params()[0].low_u64()
                    }
                }
                Hook::PostResult => {
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
                Hook::FinalBatchInfo => {
                    // set fictive l2 block
                    let txs_index = self.bootloader_state.free_tx_index();
                    let l2_block = self.bootloader_state.insert_fictive_l2_block();
                    let mut memory = vec![];
                    apply_l2_block(&mut memory, l2_block, txs_index);
                    self.write_to_bootloader_heap(memory);
                }
                Hook::PubdataRequested => {
                    if !matches!(execution_mode, VmExecutionMode::Batch) {
                        unreachable!("We do not provide the pubdata when executing the block tip or a single transaction");
                    }

                    let events =
                        merge_events(self.inner.world_diff().events(), self.batch_env.number);

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

                    apply_pubdata_to_memory(&mut memory_to_apply, pubdata_input);
                    self.write_to_bootloader_heap(memory_to_apply);
                }

                Hook::PaymasterValidationEntered | Hook::ValidationStepEnded => { /* unused */ }
                Hook::DebugLog => {
                    let (log, log_arg) = self.get_debug_log();
                    let last_tx = self.bootloader_state.last_l2_block().txs.last();
                    let tx_hash = last_tx.map(|tx| tx.hash);
                    tracing::trace!(tx = ?tx_hash, "{log}: {log_arg}");
                }
                Hook::DebugReturnData | Hook::NearCallCatch => {
                    // These hooks are for debug purposes only
                }
            }
        };

        (result, refunds)
    }

    fn get_hook_params(&self) -> [U256; 3] {
        (get_vm_hook_params_start_position(VM_VERSION)
            ..get_vm_hook_params_start_position(VM_VERSION) + VM_HOOK_PARAMS_COUNT)
            .map(|word| self.read_word_from_bootloader_heap(word as usize))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
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
        let mut bytes = [0_u8; 32];
        for offset in 0_u32..32 {
            bytes[offset as usize] = self
                .inner
                .read_heap_byte(HeapId::FIRST, start_address + offset);
        }
        U256::from_big_endian(&bytes)
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
            let mut bytes = [0_u8; 32];
            value.to_big_endian(&mut bytes);
            let start_address = slot as u32 * 32;
            for offset in 0_u32..32 {
                self.inner.write_heap_byte(
                    HeapId::FIRST,
                    start_address + offset,
                    bytes[offset as usize],
                );
            }
        }
    }

    pub(crate) fn insert_bytecodes<'a>(&mut self, bytecodes: impl IntoIterator<Item = &'a [u8]>) {
        for code in bytecodes {
            let hash = h256_to_u256(hash_bytecode(code));
            self.world.bytecode_cache.insert(hash, code.into());
        }
    }

    pub(crate) fn push_transaction_inner(
        &mut self,
        tx: zksync_types::Transaction,
        refund: u64,
        with_compression: bool,
    ) {
        let tx: TransactionData = tx.into();
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

        let memory = self.bootloader_state.push_tx(
            tx,
            overhead,
            refund,
            compressed_bytecodes,
            trusted_ergs_limit,
            self.system_env.chain_id,
        );

        self.write_to_bootloader_heap(memory);
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
        let diffs = self.inner.world_diff().get_storage_changes().map(
            move |((address, key), (initial_value, final_value))| {
                let storage_key = StorageKey::new(AccountTreeId::new(address), u256_to_h256(key));
                StateDiffRecord {
                    address,
                    key,
                    derived_key:
                        zk_evm_1_5_0::aux_structures::LogQuery::derive_final_address_for_params(
                            &address, &key,
                        ),
                    enumeration_index: storage
                        .get_enumeration_index(&storage_key)
                        .unwrap_or_default(),
                    initial_value: initial_value.unwrap_or_default(),
                    final_value,
                }
            },
        );
        diffs
            .filter(|diff| diff.address != L1_MESSENGER_ADDRESS)
            .collect()
    }

    pub(crate) fn decommitted_hashes(&self) -> impl Iterator<Item = U256> + '_ {
        self.inner.world_diff().decommitted_hashes()
    }

    pub(super) fn gas_remaining(&mut self) -> u32 {
        let frame_count = self.inner.number_of_callframes();
        for i in 0..frame_count {
            let frame = self.inner.callframe(i);
            if !frame.is_near_call() {
                return frame.gas();
            }
        }
        unreachable!("No bootloader call frame");
    }
}

// We don't implement `VmFactory` trait because, unlike old VMs, the new VM doesn't require storage to be writable;
// it maintains its own storage cache and a write buffer.
impl<S: ReadStorage> Vm<S> {
    pub fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: S) -> Self {
        let default_aa_code_hash = system_env
            .base_system_smart_contracts
            .default_aa
            .hash
            .into();

        let program_cache = HashMap::from([World::convert_system_contract_code(
            &system_env.base_system_smart_contracts.default_aa,
            false,
        )]);

        let (_, bootloader) = World::convert_system_contract_code(
            &system_env.base_system_smart_contracts.bootloader,
            true,
        );
        let bootloader_memory = bootloader_initial_memory(&batch_env);

        let mut inner = VirtualMachine::new(
            BOOTLOADER_ADDRESS,
            bootloader,
            H160::zero(),
            vec![],
            system_env.bootloader_gas_limit,
            Settings {
                default_aa_code_hash,
                // this will change after 1.5
                evm_interpreter_code_hash: default_aa_code_hash,
                hook_address: get_vm_hook_position(VM_VERSION) * 32,
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
            gas_for_account_validation: system_env.default_validation_computational_gas_limit,
            bootloader_state: BootloaderState::new(
                system_env.execution_mode,
                bootloader_memory.clone(),
                batch_env.first_l2_block,
            ),
            system_env,
            batch_env,
            snapshot: None,
            #[cfg(test)]
            enforced_state_diffs: None,
        };
        this.write_to_bootloader_heap(bootloader_memory);
        this
    }

    // visible for testing
    pub(super) fn get_current_execution_state(&self) -> CurrentExecutionState {
        let world_diff = self.inner.world_diff();
        let events = merge_events(world_diff.events(), self.batch_env.number);

        let user_l2_to_l1_logs = extract_l2tol1logs_from_l1_messenger(&events)
            .into_iter()
            .map(Into::into)
            .map(UserL2ToL1Log)
            .collect();

        CurrentExecutionState {
            events,
            deduplicated_storage_logs: world_diff
                .get_storage_changes()
                .map(|((address, key), (_, value))| StorageLog {
                    key: StorageKey::new(AccountTreeId::new(address), u256_to_h256(key)),
                    value: u256_to_h256(value),
                    kind: StorageLogKind::RepeatedWrite, // Initialness doesn't matter here
                })
                .collect(),
            used_contract_hashes: self.decommitted_hashes().collect(),
            system_logs: world_diff
                .l2_to_l1_logs()
                .iter()
                .map(|x| x.glue_into())
                .collect(),
            user_l2_to_l1_logs,
            storage_refunds: world_diff.storage_refunds().to_vec(),
            pubdata_costs: world_diff.pubdata_costs().to_vec(),
        }
    }

    fn delete_history_if_appropriate(&mut self) {
        if self.snapshot.is_none() && !self.has_previous_far_calls() {
            self.inner.delete_history();
        }
    }
}

impl<S: ReadStorage> VmInterface for Vm<S> {
    type TracerDispatcher = ();

    fn push_transaction(&mut self, tx: zksync_types::Transaction) {
        self.push_transaction_inner(tx, 0, true);
    }

    fn inspect(
        &mut self,
        (): Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        let mut track_refunds = false;
        if matches!(execution_mode, VmExecutionMode::OneTx) {
            // Move the pointer to the next transaction
            self.bootloader_state.move_tx_to_execute_pointer();
            track_refunds = true;
        }

        let mut tracer = CircuitsTracer::default();
        let start = self.inner.world_diff().snapshot();
        let pubdata_before = self.inner.world_diff().pubdata();
        let gas_before = self.gas_remaining();

        let (result, refunds) = self.run(execution_mode, &mut tracer, track_refunds);
        let ignore_world_diff = matches!(execution_mode, VmExecutionMode::OneTx)
            && matches!(result, ExecutionResult::Halt { .. });

        // If the execution is halted, the VM changes are expected to be rolled back by the caller.
        // Earlier VMs return empty execution logs in this case, so we follow this behavior.
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
                    previous_value: u256_to_h256(change.before.unwrap_or_default()),
                })
                .collect();
            let events = merge_events(
                self.inner.world_diff().events_after(&start),
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
                .map(|x| x.glue_into())
                .collect();
            VmExecutionLogs {
                storage_logs,
                events,
                user_l2_to_l1_logs,
                system_l2_to_l1_logs,
                total_log_queries_count: 0, // This field is unused
            }
        };

        let pubdata_after = self.inner.world_diff().pubdata();
        let circuit_statistic = tracer.circuit_statistic();
        let gas_remaining = self.gas_remaining();
        VmExecutionResultAndLogs {
            result,
            logs,
            // TODO (PLA-936): Fill statistics; investigate whether they should be zeroed on `Halt`
            statistics: VmExecutionStatistics {
                contracts_used: 0,
                cycles_used: 0,
                gas_used: (gas_before - gas_remaining).into(),
                gas_remaining,
                computational_gas_used: 0,
                total_log_queries: 0,
                pubdata_published: (pubdata_after - pubdata_before).max(0) as u32,
                circuit_statistic,
            },
            refunds,
        }
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        (): Self::TracerDispatcher,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        self.push_transaction_inner(tx, 0, with_compression);
        let result = self.inspect((), VmExecutionMode::OneTx);

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

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        todo!("Unused during batch execution")
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        let result = self.inspect((), VmExecutionMode::Batch);
        let execution_state = self.get_current_execution_state();
        let bootloader_memory = self.bootloader_state.bootloader_memory();
        FinishedL1Batch {
            block_tip_execution_result: result,
            final_execution_state: execution_state,
            final_bootloader_memory: Some(bootloader_memory),
            pubdata_input: Some(
                self.bootloader_state
                    .get_pubdata_information()
                    .clone()
                    .build_pubdata(false),
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
    vm_snapshot: vm2::Snapshot,
    bootloader_snapshot: BootloaderStateSnapshot,
    gas_for_account_validation: u32,
}

impl<S: ReadStorage> VmInterfaceHistoryEnabled for Vm<S> {
    fn make_snapshot(&mut self) {
        assert!(
            self.snapshot.is_none(),
            "cannot create a VM snapshot until a previous snapshot is rolled back to or popped"
        );

        self.delete_history_if_appropriate();
        self.snapshot = Some(VmSnapshot {
            vm_snapshot: self.inner.snapshot(),
            bootloader_snapshot: self.bootloader_state.get_snapshot(),
            gas_for_account_validation: self.gas_for_account_validation,
        });
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        let VmSnapshot {
            vm_snapshot,
            bootloader_snapshot,
            gas_for_account_validation,
        } = self.snapshot.take().expect("no snapshots to rollback to");

        self.inner.rollback(vm_snapshot);
        self.bootloader_state.apply_snapshot(bootloader_snapshot);
        self.gas_for_account_validation = gas_for_account_validation;

        self.delete_history_if_appropriate();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.snapshot = None;
        self.delete_history_if_appropriate();
    }
}

impl<S: fmt::Debug> fmt::Debug for Vm<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vm")
            .field(
                "gas_for_account_validation",
                &self.gas_for_account_validation,
            )
            .field("bootloader_state", &self.bootloader_state)
            .field("storage", &self.world.storage)
            .field("program_cache", &self.world.program_cache)
            .field("batch_env", &self.batch_env)
            .field("system_env", &self.system_env)
            .field("snapshot", &self.snapshot.as_ref().map(|_| ()))
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct World<S, T> {
    pub(crate) storage: S,
    program_cache: HashMap<U256, Program<T, Self>>,
    pub(crate) bytecode_cache: HashMap<U256, Vec<u8>>,
}

impl<S: ReadStorage, T: Tracer> World<S, T> {
    fn new(storage: S, program_cache: HashMap<U256, Program<T, Self>>) -> Self {
        Self {
            storage,
            program_cache,
            bytecode_cache: Default::default(),
        }
    }

    fn bytecode_to_program(bytecode: &[u8]) -> Program<T, Self> {
        Program::new(
            decode_program(
                &bytecode
                    .chunks_exact(8)
                    .map(|chunk| u64::from_be_bytes(chunk.try_into().unwrap()))
                    .collect::<Vec<_>>(),
                false,
            ),
            bytecode
                .chunks_exact(32)
                .map(U256::from_big_endian)
                .collect::<Vec<_>>(),
        )
    }

    fn convert_system_contract_code(
        code: &SystemContractCode,
        is_bootloader: bool,
    ) -> (U256, Program<T, Self>) {
        (
            h256_to_u256(code.hash),
            Program::new(
                decode_program(
                    &code
                        .code
                        .iter()
                        .flat_map(|x| x.0.into_iter().rev())
                        .collect::<Vec<_>>(),
                    is_bootloader,
                ),
                code.code.clone(),
            ),
        )
    }
}

impl<S: ReadStorage, T: Tracer> vm2::StorageInterface for World<S, T> {
    fn read_storage(&mut self, contract: H160, key: U256) -> Option<U256> {
        let key = &StorageKey::new(AccountTreeId::new(contract), u256_to_h256(key));
        if self.storage.is_write_initial(key) {
            None
        } else {
            Some(self.storage.read_value(key).as_bytes().into())
        }
    }

    fn cost_of_writing_storage(&mut self, initial_value: Option<U256>, new_value: U256) -> u32 {
        let is_initial = initial_value.is_none();
        let initial_value = initial_value.unwrap_or_default();

        if initial_value == new_value {
            return 0;
        }

        // Since we need to publish the state diffs onchain, for each of the updated storage slot
        // we basically need to publish the following pair: `(<storage_key, compressed_new_value>)`.
        // For key we use the following optimization:
        //   - The first time we publish it, we use 32 bytes.
        //         Then, we remember a 8-byte id for this slot and assign it to it. We call this initial write.
        //   - The second time we publish it, we will use the 4/5 byte representation of this 8-byte instead of the 32
        //     bytes of the entire key.
        // For value compression, we use a metadata byte which holds the length of the value and the operation from the
        // previous state to the new state, and the compressed value. The maximum for this is 33 bytes.
        // Total bytes for initial writes then becomes 65 bytes and repeated writes becomes 38 bytes.
        let compressed_value_size =
            compress_with_best_strategy(initial_value, new_value).len() as u32;

        if is_initial {
            (BYTES_PER_DERIVED_KEY as u32) + compressed_value_size
        } else {
            (BYTES_PER_ENUMERATION_INDEX as u32) + compressed_value_size
        }
    }

    fn is_free_storage_slot(&self, contract: &H160, key: &U256) -> bool {
        contract == &zksync_system_constants::SYSTEM_CONTEXT_ADDRESS
            || contract == &L2_BASE_TOKEN_ADDRESS
                && u256_to_h256(*key) == key_for_eth_balance(&BOOTLOADER_ADDRESS)
    }
}

impl<S: ReadStorage, T: Tracer> vm2::World<T> for World<S, T> {
    fn decommit(&mut self, hash: U256) -> Program<T, Self> {
        self.program_cache
            .entry(hash)
            .or_insert_with(|| {
                Self::bytecode_to_program(self.bytecode_cache.entry(hash).or_insert_with(|| {
                    self.storage
                        .load_factory_dep(u256_to_h256(hash))
                        .expect("vm tried to decommit nonexistent bytecode")
                }))
            })
            .clone()
    }

    fn decommit_code(&mut self, hash: U256) -> Vec<u8> {
        self.decommit(hash)
            .code_page()
            .as_ref()
            .iter()
            .flat_map(|u| {
                let mut buffer = [0u8; 32];
                u.to_big_endian(&mut buffer);
                buffer
            })
            .collect()
    }
}
