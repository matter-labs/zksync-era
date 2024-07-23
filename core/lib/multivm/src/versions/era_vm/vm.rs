use std::{cell::RefCell, collections::HashMap, io::Read, rc::Rc};

use era_vm::{state::VMStateBuilder, store::{InMemory, Storage, StorageError, StorageKey as EraStorageKey}, LambdaVm, VMState};
use crate::era_vm::{transaction_data::TransactionData, bytecode::compress_bytecodes};
use zksync_contracts::SystemContractCode;
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{l1::is_l1_tx_type, AccountTreeId, Address, StorageKey, Transaction, BOOTLOADER_ADDRESS, H160, U256};
use zksync_utils::{bytecode::CompressedBytecodeInfo, h256_to_u256, u256_to_h256};

use crate::{
    interface::{VmInterface, VmInterfaceHistoryEnabled}, vm_latest::{BootloaderMemory, CurrentExecutionState, ExecutionResult, HistoryEnabled, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionLogs, VmExecutionMode, VmExecutionResultAndLogs, BootloaderState}
};

use super::initial_bootloader_memory::bootloader_initial_memory;

pub struct Vm<S: ReadStorage> {
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

impl<S: ReadStorage + 'static> VmInterface<S, HistoryEnabled> for Vm<S> {
    type TracerDispatcher = ();

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let bootloader_code = system_env.base_system_smart_contracts.bootloader.code;
        let vm_state = VMState::new(bootloader_code, Vec::new(), BOOTLOADER_ADDRESS, H160::zero(), 0_u128);
        let mut pre_contract_storage = HashMap::new();
        pre_contract_storage.insert(h256_to_u256(system_env.base_system_smart_contracts.default_aa.hash), system_env.base_system_smart_contracts.default_aa.code);

        let world_storage = World::new(storage, pre_contract_storage);
        let vm = LambdaVm::new(vm_state, Box::new(world_storage));
        Self {
            inner: vm,
            suspended_at: 0,
            gas_for_account_validation: system_env.default_validation_computational_gas_limit,
            last_tx_result: None,
            bootloader_state: BootloaderState::new(
                system_env.execution_mode,
                bootloader_initial_memory(&batch_env),
                batch_env.first_l2_block
            ),
            storage,
            batch_env,
            system_env,
            snapshots: Vec::new(),
        }
        
    }

    fn push_transaction(&mut self, tx: Transaction) {
        todo!()
        // let tx: TransactionData = tx.into();
        // let overhead = tx.overhead_gas();

        // // self.insert_bytecodes(tx.factory_deps.iter().map(|dep| &dep[..]));

        // let compressed_bytecodes = if is_l1_tx_type(tx.tx_type) {
        //     // L1 transactions do not need compression
        //     vec![]
        // } else {
        //     compress_bytecodes(&tx.factory_deps, |hash| {
        //         self.inner
        //             .world
        //             .get_storage_changes()
        //             .get(&(KNOWN_CODES_STORAGE_ADDRESS.into(), h256_to_u256(hash)))
        //             .map(|x| !x.is_zero())
        //             .unwrap_or_else(|| self.storage.borrow_mut().is_bytecode_known(&hash))
        //     })
        // };

        // let trusted_ergs_limit = tx.trusted_ergs_limit();

        // let memory = self.bootloader_state.push_tx(
        //     tx,
        //     overhead,
        //     refund,
        //     compressed_bytecodes,
        //     trusted_ergs_limit,
        //     self.system_env.chain_id,
        // );
    }

    fn inspect(
        &mut self,
        _tracer: Self::TracerDispatcher,
        _execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        todo!()
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        todo!()
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        todo!()
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        todo!()
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

impl<S: ReadStorage + 'static> VmInterfaceHistoryEnabled<S> for Vm<S> {
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
pub struct World<S: ReadStorage> {
    pub storage: StoragePtr<S>,
    pub contract_storage: HashMap<U256, Vec<U256>>,
}

impl<S: ReadStorage> World<S> {
    pub fn new_empty(storage: StoragePtr<S>) -> Self {
        let contract_storage = HashMap::new();
        Self {
            contract_storage,
            storage
        }
    }

    pub fn new(storage: StoragePtr<S>, contract_storage: HashMap<U256, Vec<U256>>) -> Self {
        Self {
            storage,
            contract_storage
        }
    }
}

impl<S: ReadStorage> era_vm::store::Storage for World<S> {
    fn decommit(&self, hash: U256) -> Result<Option<Vec<U256>>, StorageError> {
        Ok(self.contract_storage.get(&hash).cloned())
    }

    fn add_contract(&mut self, hash: U256, code: Vec<U256>) -> Result<(), StorageError> {
        self.contract_storage.insert(hash, code);
        Ok(())
    }

    fn storage_read(&self, key: EraStorageKey) -> Result<Option<U256>, StorageError> {
        Ok(Some(self.storage
        .borrow_mut()
        .read_value(&StorageKey::new(
            AccountTreeId::new(key.address),
            u256_to_h256(key.key),
        ))
        .as_bytes()
        .into()))
    }

    fn storage_write(&mut self, key: EraStorageKey, value: U256) -> Result<(), StorageError> {
        todo!()
        // self.storage.borrow_mut().set_value(
        //     StorageKey::new(AccountTreeId::new(contract), u256_to_h256(key)),
        //     value.as_bytes().into(),
        // );
        // Ok(())
    }
}
