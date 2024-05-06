use std::{cell::RefCell, collections::HashMap, rc::Rc};

use vm2::{decode::decode_program, ExecutionEnd, Program, Settings, VirtualMachine};
use zk_evm_1_5_0::zkevm_opcode_defs::system_params::INITIAL_FRAME_FORMAL_EH_LOCATION;
use zksync_contracts::SystemContractCode;
use zksync_state::{ReadStorage, StoragePtr};
use zksync_types::{
    event::{extract_l2tol1logs_from_l1_messenger, extract_long_l2_to_l1_messages},
    l1::is_l1_tx_type,
    l2_to_l1_log::UserL2ToL1Log,
    utils::key_for_eth_balance,
    writes::{
        compression::compress_with_best_strategy, StateDiffRecord, BYTES_PER_DERIVED_KEY,
        BYTES_PER_ENUMERATION_INDEX,
    },
    zk_evm_types::{LogQuery, Timestamp},
    AccountTreeId, StorageKey, StorageLogQuery, BOOTLOADER_ADDRESS, H160,
    KNOWN_CODES_STORAGE_ADDRESS, L2_ETH_TOKEN_ADDRESS, U256,
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
    interface::{Halt, TxRevertReason, VmInterface, VmInterfaceHistoryEnabled, VmRevertReason},
    vm_fast::{
        bootloader_state::utils::{apply_l2_block, apply_pubdata_to_memory},
        events::merge_events,
        pubdata::PubdataInput,
        refund::compute_refund,
    },
    vm_latest::{
        constants::{
            OPERATOR_REFUNDS_OFFSET, TX_GAS_LIMIT_OFFSET, VM_HOOK_PARAMS_COUNT,
            VM_HOOK_PARAMS_START_POSITION, VM_HOOK_POSITION,
        },
        BootloaderMemory, CurrentExecutionState, ExecutionResult, HistoryEnabled, L1BatchEnv,
        L2BlockEnv, SystemEnv, VmExecutionLogs, VmExecutionMode, VmExecutionResultAndLogs,
        VmExecutionStatistics,
    },
};

pub struct Vm<S: ReadStorage> {
    pub(crate) inner: VirtualMachine,
    suspended_at: u16,
    gas_for_account_validation: u32,
    last_tx_result: Option<ExecutionResult>,

    bootloader_state: BootloaderState,
    pub(crate) storage: StoragePtr<S>,
    program_cache: Rc<RefCell<HashMap<U256, Program>>>,

    // these two are only needed for tests so far
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,

    snapshots: Vec<VmSnapshot>,
}

impl<S: ReadStorage + 'static> Vm<S> {
    fn run(&mut self, execution_mode: VmExecutionMode) -> ExecutionResult {
        loop {
            let hook = match self.inner.resume_from(self.suspended_at) {
                ExecutionEnd::SuspendedOnHook {
                    hook,
                    pc_to_resume_from,
                } => {
                    self.suspended_at = pc_to_resume_from;
                    hook
                }
                ExecutionEnd::ProgramFinished(output) => {
                    return ExecutionResult::Success { output }
                }
                ExecutionEnd::Reverted(output) => {
                    return match TxRevertReason::parse_error(&output) {
                        TxRevertReason::TxReverted(output) => ExecutionResult::Revert { output },
                        TxRevertReason::Halt(reason) => ExecutionResult::Halt { reason },
                    }
                }
                ExecutionEnd::Panicked => {
                    return ExecutionResult::Halt {
                        reason: if self.gas_remaining() == 0 {
                            Halt::BootloaderOutOfGas
                        } else {
                            Halt::VMPanic
                        },
                    }
                }
            };

            use Hook::*;
            match Hook::from_u32(hook) {
                AccountValidationEntered => self.run_account_validation(),
                PaymasterValidationEntered => {}
                AccountValidationExited => {
                    panic!("must enter account validation before exiting");
                }
                ValidationStepEnded => {}
                TxHasEnded => {
                    if let VmExecutionMode::OneTx = execution_mode {
                        return self.last_tx_result.take().unwrap();
                    }
                }
                DebugLog => {}
                DebugReturnData => {}
                NearCallCatch => {
                    todo!("NearCallCatch")
                }
                AskOperatorForRefund => {
                    let [bootloader_refund, gas_spent_on_pubdata, gas_per_pubdata_byte] =
                        self.get_hook_params();
                    let current_tx_index = self.bootloader_state.current_tx();
                    let tx_description_offset = self
                        .bootloader_state
                        .get_tx_description_offset(current_tx_index);
                    let tx_gas_limit = self
                        .read_heap_word(tx_description_offset + TX_GAS_LIMIT_OFFSET)
                        .as_u64();

                    let pubdata_published =
                        self.inner.state.current_frame.total_pubdata_spent as u32;

                    let refund = compute_refund(
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

                    self.write_to_bootloader_heap([(
                        OPERATOR_REFUNDS_OFFSET + current_tx_index,
                        refund.into(),
                    )]);
                }
                NotifyAboutRefund => {
                    let refund = self.get_hook_params()[0];
                    dbg!(refund);
                }
                PostResult => {
                    let result = self.get_hook_params()[0];

                    // TODO get latest return data
                    let return_data = vec![];

                    self.last_tx_result = Some(if result.is_zero() {
                        ExecutionResult::Revert {
                            output: VmRevertReason::from(return_data.as_slice()),
                        }
                    } else {
                        ExecutionResult::Success {
                            output: return_data,
                        }
                    });
                }
                FinalBatchInfo => {
                    // set fictive l2 block
                    let txs_index = self.bootloader_state.free_tx_index();
                    let l2_block = self.bootloader_state.insert_fictive_l2_block();
                    let mut memory = vec![];
                    apply_l2_block(&mut memory, l2_block, txs_index);
                    self.write_to_bootloader_heap(memory);
                }
                PubdataRequested => {
                    if !matches!(execution_mode, VmExecutionMode::Batch) {
                        // We do not provide the pubdata when executing the block tip or a single transaction
                        todo!("I have no idea what exit status to return here");
                    }

                    let events = merge_events(self.inner.world.events(), self.batch_env.number);
                    let pubdata_input = PubdataInput {
                        user_logs: extract_l2tol1logs_from_l1_messenger(&events),
                        l2_to_l1_messages: extract_long_l2_to_l1_messages(&events),
                        published_bytecodes: vec![], // TODO
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
            }
        }
    }

    fn run_account_validation(&mut self) {
        loop {
            match self.inner.resume_with_additional_gas_limit(
                self.suspended_at,
                self.gas_for_account_validation,
            ) {
                None => {
                    // Used too much gas
                    todo!()
                }
                Some((
                    validation_gas_left,
                    ExecutionEnd::SuspendedOnHook {
                        hook,
                        pc_to_resume_from,
                    },
                )) => {
                    self.suspended_at = pc_to_resume_from;
                    self.gas_for_account_validation = validation_gas_left;

                    let hook = Hook::from_u32(hook);
                    match hook {
                        Hook::AccountValidationExited => {
                            break;
                        }
                        Hook::DebugLog => {}
                        _ => {
                            panic!("Unexpected {:?} hook while in account validation", hook);
                        }
                    }
                }
                _ => {
                    // Exited normally without ending account validation, panicked or reverted.
                    panic!("unexpected exit from account validation")
                }
            }
        }
    }

    fn get_hook_params(&self) -> [U256; 3] {
        (VM_HOOK_PARAMS_START_POSITION..VM_HOOK_PARAMS_START_POSITION + VM_HOOK_PARAMS_COUNT)
            .map(|word| self.read_heap_word(word as usize))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    /// Typically used to read the bootloader heap. We know that we're in the bootloader
    /// when a hook occurs, as they are only enabled when preprocessing bootloader code.
    fn read_heap_word(&self, word: usize) -> U256 {
        self.inner.state.heaps[self.inner.state.current_frame.heap]
            [word as usize * 32..(word as usize + 1) * 32]
            .into()
    }

    fn write_to_bootloader_heap(&mut self, memory: impl IntoIterator<Item = (usize, U256)>) {
        assert!(self.inner.state.previous_frames.is_empty());
        let heap = &mut self.inner.state.heaps[self.inner.state.current_frame.heap];
        for (slot, value) in memory {
            let end = (slot + 1) * 32;
            if heap.len() <= end {
                heap.resize(end, 0);
            }
            value.to_big_endian(&mut heap[slot * 32..end]);
        }
    }

    pub(crate) fn insert_bytecodes<'a>(&mut self, bytecodes: impl IntoIterator<Item = &'a [u8]>) {
        for code in bytecodes {
            self.program_cache.borrow_mut().insert(
                U256::from_big_endian(hash_bytecode(code).as_bytes()),
                bytecode_to_program(code),
            );
        }
    }

    #[cfg(test)]
    /// Returns the current state of the VM in a format that can be compared for equality.
    pub(crate) fn dump_state(&self) -> (vm2::State, Vec<((H160, U256), U256)>, Box<[vm2::Event]>) {
        (
            self.inner.state.clone(),
            self.inner
                .world
                .get_storage_changes()
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect(),
            self.inner.world.events().into(),
        )
    }

    pub(crate) fn push_transaction_inner(&mut self, tx: zksync_types::Transaction, refund: u64) {
        let tx: TransactionData = tx.into();
        let overhead = tx.overhead_gas();

        self.insert_bytecodes(tx.factory_deps.iter().map(|dep| &dep[..]));

        let compressed_bytecodes = if is_l1_tx_type(tx.tx_type) {
            // L1 transactions do not need compression
            vec![]
        } else {
            compress_bytecodes(&tx.factory_deps, |hash| {
                self.inner
                    .world
                    .get_storage_changes()
                    .get(&(KNOWN_CODES_STORAGE_ADDRESS.into(), h256_to_u256(hash)))
                    .map(|x| !x.is_zero())
                    .unwrap_or_else(|| self.storage.borrow_mut().is_bytecode_known(&hash))
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

    fn compute_state_diffs(&self) -> Vec<StateDiffRecord> {
        let mut storage = self.storage.borrow_mut();

        self.inner
            .world
            .get_storage_changes()
            .iter()
            .map(|(&(address, key), value)| {
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
                    initial_value: storage.read_value(&storage_key).as_bytes().into(),
                    final_value: *value,
                }
            })
            .collect()
    }
}

impl<S: ReadStorage + 'static> VmInterface<S, HistoryEnabled> for Vm<S> {
    type TracerDispatcher = ();

    fn new(
        batch_env: crate::vm_latest::L1BatchEnv,
        system_env: crate::vm_latest::SystemEnv,
        storage: StoragePtr<S>,
    ) -> Self {
        let default_aa_code_hash = system_env
            .base_system_smart_contracts
            .default_aa
            .hash
            .into();

        let program_cache = Rc::new(RefCell::new(HashMap::from([convert_system_contract_code(
            &system_env.base_system_smart_contracts.default_aa,
            false,
        )])));

        let (_, bootloader) =
            convert_system_contract_code(&system_env.base_system_smart_contracts.bootloader, true);
        let bootloader_memory = bootloader_initial_memory(&batch_env);

        let mut inner = VirtualMachine::new(
            Box::new(World::new(storage.clone(), program_cache.clone())),
            BOOTLOADER_ADDRESS,
            bootloader,
            H160::zero(),
            vec![],
            system_env.bootloader_gas_limit,
            Settings {
                default_aa_code_hash,
                // this will change after 1.5
                evm_interpreter_code_hash: default_aa_code_hash,
                hook_address: VM_HOOK_POSITION * 32,
            },
        );

        inner.state.current_frame.sp = 0;

        // The bootloader shouldn't pay for growing memory and it writes results
        // to the end of its heap, so it makes sense to preallocate it in its entirety.
        const BOOTLOADER_MAX_MEMORY_SIZE: usize = 59000000;
        inner.state.heaps[vm2::FIRST_HEAP].resize(BOOTLOADER_MAX_MEMORY_SIZE, 0);
        inner.state.heaps[vm2::FIRST_HEAP + 1].resize(BOOTLOADER_MAX_MEMORY_SIZE, 0);

        inner.state.current_frame.exception_handler = INITIAL_FRAME_FORMAL_EH_LOCATION;

        let mut me = Self {
            inner,
            suspended_at: 0,
            gas_for_account_validation: system_env.default_validation_computational_gas_limit,
            last_tx_result: None,
            bootloader_state: BootloaderState::new(
                system_env.execution_mode,
                bootloader_memory.clone(),
                batch_env.first_l2_block,
            ),
            storage,
            system_env,
            batch_env,
            program_cache,
            snapshots: vec![],
        };

        me.write_to_bootloader_heap(bootloader_memory);

        me
    }

    fn push_transaction(&mut self, tx: zksync_types::Transaction) {
        self.push_transaction_inner(tx, 0);
    }

    fn inspect(
        &mut self,
        _dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        let mut enable_refund_tracer = false;
        if let VmExecutionMode::OneTx = execution_mode {
            // Move the pointer to the next transaction
            self.bootloader_state.move_tx_to_execute_pointer();
            enable_refund_tracer = true;
        }

        let start = self.inner.world.snapshot();
        let pubdata_before = self.inner.state.current_frame.total_pubdata_spent;

        let result = self.run(execution_mode);

        let events = merge_events(self.inner.world.events_after(&start), self.batch_env.number);
        let user_l2_to_l1_logs = extract_l2tol1logs_from_l1_messenger(&events)
            .into_iter()
            .map(Into::into)
            .map(UserL2ToL1Log)
            .collect();
        let pubdata_after = self.inner.state.current_frame.total_pubdata_spent;

        VmExecutionResultAndLogs {
            result,
            logs: VmExecutionLogs {
                storage_logs: self
                    .inner
                    .world
                    .get_storage_changes_after(&start)
                    .into_iter()
                    .map(|((address, key), (before, after))| StorageLogQuery {
                        log_query: LogQuery {
                            timestamp: Timestamp(0),
                            tx_number_in_block: 0, // incorrect and hopefully unused
                            aux_byte: 0,           // incorrect and hopefully unused
                            shard_id: 0,
                            address,
                            key,
                            read_value: before.unwrap_or_default(),
                            written_value: after,
                            rw_flag: true,
                            rollback: false,
                            is_service: false, // incorrect and hopefully unused
                        },
                        log_type: zksync_types::StorageLogQueryType::RepeatedWrite, // incorrect and hopefully unused
                    })
                    .collect(),
                events,
                user_l2_to_l1_logs,
                system_l2_to_l1_logs: self
                    .inner
                    .world
                    .l2_to_l1_logs_after(&start)
                    .iter()
                    .map(|x| x.glue_into())
                    .collect(),
                total_log_queries_count: 0, // This field is unused
            },
            statistics: VmExecutionStatistics {
                contracts_used: 0,         // TODO
                cycles_used: 0,            // TODO
                gas_used: 0,               // TODO
                gas_remaining: 0,          // TODO
                computational_gas_used: 0, // TODO
                total_log_queries: 0,      // TODO
                pubdata_published: (pubdata_after - pubdata_before).max(0) as u32,
                circuit_statistic: Default::default(), // TODO
            },
            refunds: Default::default(),
        }
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> (
        Result<(), crate::interface::BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        todo!()
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        self.bootloader_state.bootloader_memory()
    }

    fn get_last_tx_compressed_bytecodes(
        &self,
    ) -> Vec<zksync_utils::bytecode::CompressedBytecodeInfo> {
        self.bootloader_state.get_last_tx_compressed_bytecodes()
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.bootloader_state.start_new_l2_block(l2_block_env)
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        // TODO fill all fields
        CurrentExecutionState {
            events: vec![],
            storage_log_queries: vec![],
            deduplicated_storage_log_queries: vec![],
            used_contract_hashes: vec![],
            system_logs: vec![],
            user_l2_to_l1_logs: vec![],
            total_log_queries: 0,
            cycles_used: 0,
            deduplicated_events_logs: vec![],
            storage_refunds: vec![],
            pubdata_costs: vec![],
        }
    }

    fn record_vm_memory_metrics(&self) -> crate::vm_latest::VmMemoryMetrics {
        todo!()
    }

    fn gas_remaining(&self) -> u32 {
        self.inner.state.current_frame.gas
    }
}

struct VmSnapshot {
    vm_snapshot: vm2::Snapshot,
    bootloader_snapshot: BootloaderStateSnapshot,
    suspended_at: u16,
    gas_for_account_validation: u32,
}

impl<S: ReadStorage + 'static> VmInterfaceHistoryEnabled<S> for Vm<S> {
    fn make_snapshot(&mut self) {
        self.snapshots.push(VmSnapshot {
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
        } = self.snapshots.pop().expect("no snapshots to rollback to");

        self.inner.rollback(vm_snapshot);
        self.bootloader_state.apply_snapshot(bootloader_snapshot);
        self.suspended_at = suspended_at;
        self.gas_for_account_validation = gas_for_account_validation;

        self.delete_history_if_appropriate();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.snapshots.pop();
        self.delete_history_if_appropriate();
    }
}

impl<S: ReadStorage + 'static> Vm<S> {
    fn delete_history_if_appropriate(&mut self) {
        if self.snapshots.is_empty() && self.inner.state.previous_frames.is_empty() {
            self.inner.world.delete_history();
        }
    }
}

struct World<S: ReadStorage> {
    storage: StoragePtr<S>,

    // TODO: It would be nice to store an LRU cache elsewhere.
    // This one is cleared on change of batch unfortunately.
    program_cache: Rc<RefCell<HashMap<U256, Program>>>,
}

impl<S: ReadStorage> World<S> {
    fn new(storage: StoragePtr<S>, program_cache: Rc<RefCell<HashMap<U256, Program>>>) -> Self {
        Self {
            storage,
            program_cache,
        }
    }
}

impl<S: ReadStorage> vm2::World for World<S> {
    fn decommit(&mut self, hash: U256) -> Program {
        self.program_cache
            .borrow_mut()
            .entry(hash)
            .or_insert_with(|| {
                let bytecode = self
                    .storage
                    .borrow_mut()
                    .load_factory_dep(u256_to_h256(hash))
                    .expect("vm tried to decommit nonexistent bytecode");

                bytecode_to_program(&bytecode)
            })
            .clone()
    }

    fn read_storage(&mut self, contract: zksync_types::H160, key: U256) -> U256 {
        self.storage
            .borrow_mut()
            .read_value(&StorageKey::new(
                AccountTreeId::new(contract),
                u256_to_h256(key),
            ))
            .as_bytes()
            .into()
    }

    fn cost_of_writing_storage(&mut self, contract: H160, key: U256, new_value: U256) -> u32 {
        let storage_key = StorageKey::new(AccountTreeId::new(contract), u256_to_h256(key));

        let initial_value = h256_to_u256(self.storage.borrow_mut().read_value(&storage_key));

        if initial_value == new_value {
            return 0;
        }

        let is_initial = self.storage.borrow_mut().is_write_initial(&storage_key);

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
        // TODO (SMA-1702): take into account the content of the log query, i.e. values that contain mostly zeroes
        // should cost less.

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
            || contract == &L2_ETH_TOKEN_ADDRESS
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
