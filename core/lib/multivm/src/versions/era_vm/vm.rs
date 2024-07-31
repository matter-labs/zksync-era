use std::{borrow::Borrow, cell::RefCell, collections::HashMap, fmt::Write, io::Read, rc::Rc};

use era_vm::{
    state::{Event, VMStateBuilder},
    store::{InMemory, L2ToL1Log, Storage, StorageError, StorageKey as EraStorageKey},
    vm::ExecutionOutput,
    LambdaVm, VMState,
};
use zksync_contracts::SystemContractCode;
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{
    l1::is_l1_tx_type, AccountTreeId, Address, L1BatchNumber, StorageKey, Transaction, VmEvent,
    BOOTLOADER_ADDRESS, H160, KNOWN_CODES_STORAGE_ADDRESS, U256,
};
use zksync_utils::{bytecode::CompressedBytecodeInfo, h256_to_u256, u256_to_h256};

use super::{
    bootloader_state::BootloaderState, event::merge_events,
    initial_bootloader_memory::bootloader_initial_memory, snapshot::VmSnapshot,
};
use crate::{
    era_vm::{bytecode::compress_bytecodes, transaction_data::TransactionData},
    interface::{Halt, TxRevertReason, VmInterface, VmInterfaceHistoryEnabled},
    vm_latest::{
        BootloaderMemory, CurrentExecutionState, ExecutionResult, HistoryEnabled, L1BatchEnv,
        L2BlockEnv, SystemEnv, VmExecutionLogs, VmExecutionMode, VmExecutionResultAndLogs,
    },
    HistoryMode,
};

pub struct Vm<S: WriteStorage> {
    pub(crate) inner: LambdaVm,
    suspended_at: u16,
    gas_for_account_validation: u32,
    last_tx_result: Option<ExecutionResult>,

    bootloader_state: BootloaderState,
    pub(crate) storage: StoragePtr<S>,

    // TODO: Maybe not necessary, check
    // program_cache: Rc<RefCell<HashMap<U256, Vec<U256>>>>,

    // these two are only needed for tests so far
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,

    snapshots: Vec<VmSnapshot>, // TODO: Implement snapshots logic
}

impl<S: WriteStorage + 'static> Vm<S> {
    pub fn run(&mut self, _execution_mode: VmExecutionMode) -> (ExecutionResult, VMState) {
        let (result, final_vm) = self.inner.run_program_with_custom_bytecode();
        let result = match result {
            ExecutionOutput::Ok(output) => {
                // println!("ExecutionOutput::Ok");
                ExecutionResult::Success { output }
            }
            ExecutionOutput::Revert(output) => {
                // println!("ExecutionOutput::Revert");
                match TxRevertReason::parse_error(&output) {
                    TxRevertReason::TxReverted(output) => ExecutionResult::Revert { output },
                    TxRevertReason::Halt(reason) => ExecutionResult::Halt { reason },
                }
            }
            ExecutionOutput::Panic => ExecutionResult::Halt {
                reason: if self.inner.state.gas_left().unwrap() == 0 {
                    Halt::BootloaderOutOfGas
                } else {
                    Halt::VMPanic
                },
            },
        };
        (result, final_vm)
    }

    fn write_to_bootloader_heap(&mut self, memory: impl IntoIterator<Item = (usize, U256)>) {
        assert!(self.inner.state.running_contexts.len() == 1); // No on-going far calls
        if let Some(heap) = &mut self
            .inner
            .state
            .heaps
            .get(self.inner.state.current_frame().unwrap().heap_id)
            .cloned()
        {
            for (slot, value) in memory {
                let end = (slot + 1) * 32;
                if heap.len() <= end {
                    heap.expand_memory(end as u32);
                }
                heap.store((slot * 32) as u32, value);
            }
        }
    }
}

impl<S: WriteStorage + 'static> VmInterface<S, HistoryEnabled> for Vm<S> {
    type TracerDispatcher = ();

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let bootloader_code = system_env
            .clone()
            .base_system_smart_contracts
            .bootloader
            .code;
        let vm_state = VMState::new(
            bootloader_code.to_owned(),
            Vec::new(),
            BOOTLOADER_ADDRESS,
            H160::zero(),
            0_u128,
        );
        let mut pre_contract_storage = HashMap::new();
        pre_contract_storage.insert(
            h256_to_u256(
                system_env
                    .clone()
                    .base_system_smart_contracts
                    .default_aa
                    .hash,
            ),
            system_env
                .clone()
                .base_system_smart_contracts
                .default_aa
                .code,
        );

        let world_storage = World::new(storage.clone(), pre_contract_storage);
        let mut vm = LambdaVm::new(vm_state, Rc::new(RefCell::new(world_storage)));
        let bootloader_memory = bootloader_initial_memory(&batch_env);
        let mut mv = Self {
            inner: vm,
            suspended_at: 0,
            gas_for_account_validation: system_env
                .clone()
                .default_validation_computational_gas_limit,
            last_tx_result: None,
            bootloader_state: BootloaderState::new(
                system_env.execution_mode.clone(),
                bootloader_initial_memory(&batch_env),
                batch_env.first_l2_block,
            ),
            storage,
            batch_env,
            system_env,
            snapshots: Vec::new(),
        };

        mv.write_to_bootloader_heap(bootloader_memory);
        mv
    }

    fn push_transaction(&mut self, tx: Transaction) {
        let tx: TransactionData = tx.into();
        let overhead = tx.overhead_gas();

        // self.insert_bytecodes(tx.factory_deps.iter().map(|dep| &dep[..]));

        let compressed_bytecodes = if is_l1_tx_type(tx.tx_type) {
            // L1 transactions do not need compression
            vec![]
        } else {
            compress_bytecodes(&tx.factory_deps, |hash| {
                (*self.inner.storage.clone().borrow_mut())
                    .storage_read(EraStorageKey::new(
                        KNOWN_CODES_STORAGE_ADDRESS,
                        h256_to_u256(hash),
                    ))
                    .map(|x| !x.is_none())
                    .unwrap_or_else(|_| {
                        let mut storage = RefCell::borrow_mut(&self.storage);
                        storage.is_bytecode_known(&hash)
                    })
            })
        };

        let trusted_ergs_limit = tx.trusted_ergs_limit();

        let memory = self.bootloader_state.push_tx(
            tx,
            overhead,
            0, //TODO: Is this correct?
            compressed_bytecodes,
            trusted_ergs_limit,
            self.system_env.chain_id,
        );

        self.write_to_bootloader_heap(memory);
    }

    fn inspect(
        &mut self,
        _tracer: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        let mut enable_refund_tracer = false;
        if let VmExecutionMode::OneTx = execution_mode {
            // Move the pointer to the next transaction
            self.bootloader_state.move_tx_to_execute_pointer();
            enable_refund_tracer = true;
        }

        let (result, final_vm_state) = self.run(execution_mode);
        //dbg!(&result);

        VmExecutionResultAndLogs {
            result,
            logs: VmExecutionLogs {
                storage_logs: Default::default(),
                events: merge_events(&final_vm_state.events, self.batch_env.number),
                user_l2_to_l1_logs: Default::default(),
                system_l2_to_l1_logs: Default::default(),
                total_log_queries_count: 0, // This field is unused
            },
            statistics: Default::default(),
            refunds: Default::default(),
        }
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
        todo!()
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

    fn record_vm_memory_metrics(&self) -> crate::vm_1_4_1::VmMemoryMetrics {
        todo!()
    }

    fn gas_remaining(&self) -> u32 {
        todo!()
    }
}

impl<S: WriteStorage + 'static> VmInterfaceHistoryEnabled<S> for Vm<S> {
    fn make_snapshot(&mut self) {
        todo!()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        todo!()
    }

    fn pop_snapshot_no_rollback(&mut self) {
        todo!()
    }
}

#[derive(Debug)]
pub struct World<S: WriteStorage> {
    pub storage: StoragePtr<S>,
    pub contract_storage: HashMap<U256, Vec<U256>>,
}

impl<S: WriteStorage> World<S> {
    pub fn new_empty(storage: StoragePtr<S>) -> Self {
        let contract_storage = HashMap::new();
        Self {
            contract_storage,
            storage,
        }
    }

    pub fn new(storage: StoragePtr<S>, contract_storage: HashMap<U256, Vec<U256>>) -> Self {
        Self {
            storage,
            contract_storage,
        }
    }
}

impl<S: WriteStorage> era_vm::store::Storage for World<S> {
    fn decommit(&self, hash: U256) -> Result<Option<Vec<U256>>, StorageError> {
        let contract = self.contract_storage.get(&hash).cloned();
        if contract.is_none() {
            let contract = self.storage.borrow_mut().load_factory_dep(u256_to_h256(hash)).expect("Bytecode not found");
            let mut program_code = vec![];
            for raw_opcode_slice in contract.chunks(32) {
                let mut raw_opcode_bytes: [u8; 32] = [0; 32];
                raw_opcode_bytes.copy_from_slice(&raw_opcode_slice[..32]);

                let raw_opcode_u256 = U256::from_big_endian(&raw_opcode_bytes);
                program_code.push(raw_opcode_u256);
            }
            return Ok(Some(program_code));
        }
        Ok(contract)
    }

    fn add_contract(&mut self, hash: U256, code: Vec<U256>) -> Result<(), StorageError> {
        self.contract_storage.insert(hash, code);
        Ok(())
    }

    fn storage_read(&self, key: EraStorageKey) -> Result<Option<U256>, StorageError> {
        let mut storage = RefCell::borrow_mut(&self.storage);
        Ok(Some(
            storage
                .read_value(&StorageKey::new(
                    AccountTreeId::new(key.address),
                    u256_to_h256(key.key),
                ))
                .as_bytes()
                .into(),
        ))
    }

    fn storage_write(&mut self, key: EraStorageKey, value: U256) -> Result<(), StorageError> {
        let mut storage = RefCell::borrow_mut(&self.storage);
        storage.set_value(
            StorageKey::new(AccountTreeId::new(key.address), u256_to_h256(key.key)),
            u256_to_h256(value),
        );
        Ok(())
    }

    fn get_state_storage(&self) -> &HashMap<EraStorageKey, U256> {
        unimplemented!()
    }
}
