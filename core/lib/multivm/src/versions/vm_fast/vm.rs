use std::{collections::HashMap, fmt};

use vm2::{
    decode::decode_program, fat_pointer::FatPointer, instruction_handlers::HeapInterface,
    ExecutionEnd, Program, Settings, VirtualMachine,
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
    hook::Hook,
    initial_bootloader_memory::bootloader_initial_memory,
    transaction_data::TransactionData,
};
use crate::{
    glue::GlueInto,
    interface::{
        storage::ReadStorage, BootloaderMemory, BytecodeCompressionError, CompressedBytecodeInfo,
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
    pub(crate) world: World<S>,
    pub(crate) inner: VirtualMachine,
    suspended_at: u16,
    gas_for_account_validation: u32,
    pub(crate) bootloader_state: BootloaderState,
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,
    snapshot: Option<VmSnapshot>,
}

impl<S: ReadStorage> Vm<S> {
    fn run(
        &mut self,
        execution_mode: VmExecutionMode,
        track_refunds: bool,
    ) -> (ExecutionResult, Refunds) {
        let mut refunds = Refunds {
            gas_refunded: 0,
            operator_suggested_refund: 0,
        };
        let mut last_tx_result = None;
        let mut pubdata_before = self.inner.world_diff.pubdata() as u32;

        let result = loop {
            let hook = match self.inner.resume_from(self.suspended_at, &mut self.world) {
                ExecutionEnd::SuspendedOnHook {
                    hook,
                    pc_to_resume_from,
                } => {
                    self.suspended_at = pc_to_resume_from;
                    hook
                }
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

                        let pubdata_published = self.inner.world_diff.pubdata() as u32;

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
                    assert_eq!(fp.offset, 0);

                    let return_data = self.inner.state.heaps[fp.memory_page]
                        .read_range_big_endian(fp.start..fp.start + fp.length);

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
                        merge_events(self.inner.world_diff.events(), self.batch_env.number);

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
                        state_diffs: self
                            .compute_state_diffs()
                            .filter(|diff| diff.address != L1_MESSENGER_ADDRESS)
                            .collect(),
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
                Hook::DebugLog | Hook::DebugReturnData | Hook::NearCallCatch => {
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

    /// Should only be used when the bootloader is executing (e.g., when handling hooks).
    pub(crate) fn read_word_from_bootloader_heap(&self, word: usize) -> U256 {
        self.inner.state.heaps[vm2::FIRST_HEAP].read_u256(word as u32 * 32)
    }

    /// Should only be used when the bootloader is executing (e.g., when handling hooks).
    pub(crate) fn write_to_bootloader_heap(
        &mut self,
        memory: impl IntoIterator<Item = (usize, U256)>,
    ) {
        assert!(self.inner.state.previous_frames.is_empty());
        for (slot, value) in memory {
            self.inner
                .state
                .heaps
                .write_u256(vm2::FIRST_HEAP, slot as u32 * 32, value);
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
                    .world_diff
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

    fn compute_state_diffs(&mut self) -> impl Iterator<Item = StateDiffRecord> + '_ {
        let storage = &mut self.world.storage;

        self.inner.world_diff.get_storage_changes().map(
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
        )
    }

    pub(crate) fn decommitted_hashes(&self) -> impl Iterator<Item = U256> + '_ {
        self.inner.world_diff.decommitted_hashes()
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

        let program_cache = HashMap::from([convert_system_contract_code(
            &system_env.base_system_smart_contracts.default_aa,
            false,
        )]);

        let (_, bootloader) =
            convert_system_contract_code(&system_env.base_system_smart_contracts.bootloader, true);
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

        inner.state.current_frame.sp = 0;

        // The bootloader writes results to high addresses in its heap, so it makes sense to preallocate it.
        inner.state.current_frame.heap_size = u32::MAX;
        inner.state.current_frame.aux_heap_size = u32::MAX;
        inner.state.current_frame.exception_handler = INITIAL_FRAME_FORMAL_EH_LOCATION;

        let mut me = Self {
            world: World::new(storage, program_cache),
            inner,
            suspended_at: 0,
            gas_for_account_validation: system_env.default_validation_computational_gas_limit,
            bootloader_state: BootloaderState::new(
                system_env.execution_mode,
                bootloader_memory.clone(),
                batch_env.first_l2_block,
            ),
            system_env,
            batch_env,
            snapshot: None,
        };

        me.write_to_bootloader_heap(bootloader_memory);

        me
    }

    fn delete_history_if_appropriate(&mut self) {
        if self.snapshot.is_none() && self.inner.state.previous_frames.is_empty() {
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

        let start = self.inner.world_diff.snapshot();
        let pubdata_before = self.inner.world_diff.pubdata();

        let (result, refunds) = self.run(execution_mode, track_refunds);
        let ignore_world_diff = matches!(execution_mode, VmExecutionMode::OneTx)
            && matches!(result, ExecutionResult::Halt { .. });

        // If the execution is halted, the VM changes are expected to be rolled back by the caller.
        // Earlier VMs return empty execution logs in this case, so we follow this behavior.
        let logs = if ignore_world_diff {
            VmExecutionLogs::default()
        } else {
            let storage_logs = self
                .inner
                .world_diff
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
                self.inner.world_diff.events_after(&start),
                self.batch_env.number,
            );
            let user_l2_to_l1_logs = extract_l2tol1logs_from_l1_messenger(&events)
                .into_iter()
                .map(Into::into)
                .map(UserL2ToL1Log)
                .collect();
            let system_l2_to_l1_logs = self
                .inner
                .world_diff
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

        let pubdata_after = self.inner.world_diff.pubdata();
        VmExecutionResultAndLogs {
            result,
            logs,
            // TODO (PLA-936): Fill statistics; investigate whether they should be zeroed on `Halt`
            statistics: VmExecutionStatistics {
                contracts_used: 0,
                cycles_used: 0,
                gas_used: 0,
                gas_remaining: 0,
                computational_gas_used: 0,
                total_log_queries: 0,
                pubdata_published: (pubdata_after - pubdata_before).max(0) as u32,
                circuit_statistic: Default::default(),
            },
            refunds,
        }
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        (): Self::TracerDispatcher,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        self.push_transaction_inner(tx, 0, with_compression);
        let result = self.inspect((), VmExecutionMode::OneTx);

        let compression_result = if self.has_unpublished_bytecodes() {
            Err(BytecodeCompressionError::BytecodeCompressionFailed)
        } else {
            Ok(())
        };
        (compression_result, result)
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        self.bootloader_state.bootloader_memory()
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        self.bootloader_state.get_last_tx_compressed_bytecodes()
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.bootloader_state.start_new_l2_block(l2_block_env)
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        let world_diff = &self.inner.world_diff;
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

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        todo!("Unused during batch execution")
    }

    fn gas_remaining(&self) -> u32 {
        self.inner.state.current_frame.gas
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        let result = self.execute(VmExecutionMode::Batch);
        let execution_state = self.get_current_execution_state();
        let bootloader_memory = self.get_bootloader_memory();
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
    suspended_at: u16,
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
            suspended_at: self.suspended_at,
            gas_for_account_validation: self.gas_for_account_validation,
        });
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        let VmSnapshot {
            vm_snapshot,
            bootloader_snapshot,
            suspended_at,
            gas_for_account_validation,
        } = self.snapshot.take().expect("no snapshots to rollback to");

        self.inner.rollback(vm_snapshot);
        self.bootloader_state.apply_snapshot(bootloader_snapshot);
        self.suspended_at = suspended_at;
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
            .field("suspended_at", &self.suspended_at)
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
pub(crate) struct World<S> {
    pub(crate) storage: S,
    // TODO (PLA-1008): Store `Program`s in an LRU cache
    program_cache: HashMap<U256, Program>,
    pub(crate) bytecode_cache: HashMap<U256, Vec<u8>>,
}

impl<S: ReadStorage> World<S> {
    fn new(storage: S, program_cache: HashMap<U256, Program>) -> Self {
        Self {
            storage,
            program_cache,
            bytecode_cache: Default::default(),
        }
    }
}

impl<S: ReadStorage> vm2::World for World<S> {
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

    fn decommit(&mut self, hash: U256) -> Program {
        self.program_cache
            .entry(hash)
            .or_insert_with(|| {
                bytecode_to_program(self.bytecode_cache.entry(hash).or_insert_with(|| {
                    self.storage
                        .load_factory_dep(u256_to_h256(hash))
                        .expect("vm tried to decommit nonexistent bytecode")
                }))
            })
            .clone()
    }

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

fn bytecode_to_program(bytecode: &[u8]) -> Program {
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

fn convert_system_contract_code(code: &SystemContractCode, is_bootloader: bool) -> (U256, Program) {
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
