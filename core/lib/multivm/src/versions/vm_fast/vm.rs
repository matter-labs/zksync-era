use std::{cell::RefCell, collections::HashMap, rc::Rc};

use vm2::{decode::decode_program, Event, ExecutionEnd, Program, Settings, State, VirtualMachine};
use zk_evm_1_5_0::zkevm_opcode_defs::system_params::INITIAL_FRAME_FORMAL_EH_LOCATION;
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{l1::is_l1_tx_type, AccountTreeId, StorageKey, BOOTLOADER_ADDRESS, H160, U256};
use zksync_utils::bytecode::hash_bytecode;

use crate::{
    interface::{VmInterface, VmInterfaceHistoryEnabled},
    vm_latest::{
        constants::VM_HOOK_POSITION, BootloaderMemory, CurrentExecutionState, HistoryEnabled,
        L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode, VmExecutionResultAndLogs,
    },
};

use super::{
    bootloader_state::{BootloaderState, BootloaderStateSnapshot},
    bytecode::compress_bytecodes,
    initial_bootloader_memory::bootloader_initial_memory,
    transaction_data::TransactionData,
};

pub struct Vm<S: WriteStorage> {
    pub(crate) inner: VirtualMachine,
    suspended_at: u16,
    gas_for_account_validation: u32,

    bootloader_state: BootloaderState,
    storage: StoragePtr<S>,
    program_cache: Rc<RefCell<HashMap<U256, Program>>>,

    // these two are only needed for tests so far
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,

    snapshots: Vec<VmSnapshot>,
}

impl<S: WriteStorage> Vm<S> {
    fn run(&mut self) {
        loop {
            let end = self.inner.resume_from(self.suspended_at);
            let ExecutionEnd::SuspendedOnHook {
                hook,
                pc_to_resume_from,
            } = end
            else {
                panic!("expected hook")
            };

            self.suspended_at = pc_to_resume_from;

            match hook {
                0 => {
                    // Account validation entered
                    match self.inner.resume_with_additional_gas_limit(
                        pc_to_resume_from,
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
                            assert_eq!(hook, 2, "Should not hook while in account validation");
                            self.gas_for_account_validation = validation_gas_left;
                            self.suspended_at = pc_to_resume_from;
                        }
                        _ => {
                            // Exited normally without ending account validation, panicked or reverted.
                            panic!("unexpected exit from account validation")
                        }
                    }
                }
                1 => {} // PaymasterValidationEntered,
                2 => {
                    // Account validation exited
                    panic!("must enter account validation before exiting");
                }
                3 => {}  // ValidationStepEnded,
                4 => {}  // TxHasEnded,
                5 => {}  // DebugLog,
                6 => {}  // DebugReturnData,
                7 => {}  // NearCallCatch,
                8 => {}  // AskOperatorForRefund,
                9 => {}  // NotifyAboutRefund,
                10 => {} // ExecutionResult,
                11 => {} // FinalBatchInfo,
                12 => {} // PubdataRequested,
                _ => panic!("Unknown hook: {}", hook),
            }
        }
    }

    fn write_to_bootloader_heap(&mut self, memory: Vec<(usize, U256)>) {
        //assert!(self.inner.state.previous_frames.is_empty());
        let heap = &mut self.inner.state.heaps[self.inner.state.current_frame.heap];
        for (slot, value) in memory {
            value.to_big_endian(&mut heap[slot * 32..(slot + 1) * 32]);
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
    pub(crate) fn dump_state(&self) -> (State, Vec<((H160, U256), U256)>, Box<[Event]>) {
        (
            self.inner.state.clone(),
            self.inner.world.get_storage_changes().collect(),
            self.inner.world.events().into(),
        )
    }
}

impl<S: WriteStorage + 'static> VmInterface<S, HistoryEnabled> for Vm<S> {
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

        let program_cache = Rc::new(RefCell::new(HashMap::new()));

        let mut inner = VirtualMachine::new(
            Box::new(World::new(storage.clone(), program_cache.clone())),
            BOOTLOADER_ADDRESS,
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
        // TODO: bootloader should not pay for memory growth but with how vm2 currently works that would allocate 4GB
        inner.state.current_frame.exception_handler = INITIAL_FRAME_FORMAL_EH_LOCATION;

        Self {
            inner,
            suspended_at: 0,
            gas_for_account_validation: system_env.default_validation_computational_gas_limit,
            bootloader_state: BootloaderState::new(
                system_env.execution_mode,
                bootloader_initial_memory(&batch_env),
                batch_env.first_l2_block,
            ),
            storage,
            system_env,
            batch_env,
            program_cache,
            snapshots: vec![],
        }
    }

    fn push_transaction(&mut self, tx: zksync_types::Transaction) {
        let tx: TransactionData = tx.into();
        let overhead = tx.overhead_gas();

        self.insert_bytecodes(tx.factory_deps.iter().map(|dep| &dep[..]));

        let compressed_bytecodes = if is_l1_tx_type(tx.tx_type) {
            // L1 transactions do not need compression
            vec![]
        } else {
            compress_bytecodes(&tx.factory_deps, self.storage.clone())
        };

        let trusted_ergs_limit = tx.trusted_ergs_limit();

        let memory = self.bootloader_state.push_tx(
            tx,
            overhead,
            0,
            compressed_bytecodes,
            trusted_ergs_limit,
            self.system_env.chain_id,
        );

        self.write_to_bootloader_heap(memory);
    }

    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
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
        todo!()
    }

    fn record_vm_memory_metrics(&self) -> crate::vm_latest::VmMemoryMetrics {
        todo!()
    }

    fn gas_remaining(&self) -> u32 {
        self.inner.state.current_frame.gas
    }
}

struct VmSnapshot {
    state: vm2::State,
    world_snapshot: vm2::ExternalSnapshot,
    bootloader_snapshot: BootloaderStateSnapshot,
    suspended_at: u16,
    gas_for_account_validation: u32,
}

impl<S: WriteStorage + 'static> VmInterfaceHistoryEnabled<S> for Vm<S> {
    fn make_snapshot(&mut self) {
        self.snapshots.push(VmSnapshot {
            state: self.inner.state.clone(),
            world_snapshot: self.inner.world.external_snapshot(),
            bootloader_snapshot: self.bootloader_state.get_snapshot(),
            suspended_at: self.suspended_at,
            gas_for_account_validation: self.gas_for_account_validation,
        });
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        let VmSnapshot {
            state,
            world_snapshot,
            bootloader_snapshot,
            suspended_at,
            gas_for_account_validation,
        } = self.snapshots.pop().expect("no snapshots to rollback to");

        self.inner.state = state;
        self.inner.world.external_rollback(world_snapshot);
        self.bootloader_state.apply_snapshot(bootloader_snapshot);
        self.suspended_at = suspended_at;
        self.gas_for_account_validation = gas_for_account_validation;
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.snapshots.pop();
        self.delete_history_if_appropriate();
    }
}

impl<S: WriteStorage + 'static> Vm<S> {
    fn delete_history_if_appropriate(&mut self) {
        todo!()
    }
}

struct World<S: WriteStorage> {
    storage: StoragePtr<S>,

    // TODO: It would be nice to store an LRU cache elsewhere.
    // This one is cleared on change of batch unfortunately.
    program_cache: Rc<RefCell<HashMap<U256, Program>>>,
}

impl<S: WriteStorage> World<S> {
    fn new(storage: StoragePtr<S>, program_cache: Rc<RefCell<HashMap<U256, Program>>>) -> Self {
        Self {
            storage,
            program_cache,
        }
    }
}

impl<S: WriteStorage> vm2::World for World<S> {
    fn decommit(&mut self, hash: U256) -> Program {
        self.program_cache
            .borrow_mut()
            .entry(hash)
            .or_insert_with(|| {
                let mut hash_bytes = [0; 32];
                hash.to_big_endian(&mut hash_bytes);

                let bytecode = self
                    .storage
                    .borrow_mut()
                    .load_factory_dep(hash_bytes.into())
                    .expect("vm tried to decommit nonexistent bytecode");

                bytecode_to_program(&bytecode)
            })
            .clone()
    }

    fn read_storage(&mut self, contract: zksync_types::H160, key: U256) -> U256 {
        let mut key_bytes = [0; 32];
        key.to_big_endian(&mut key_bytes);

        self.storage
            .borrow_mut()
            .read_value(&StorageKey::new(
                AccountTreeId::new(contract.into()),
                key_bytes.into(),
            ))
            .as_bytes()
            .into()
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
            .map(|chunk| U256::from_big_endian(chunk.try_into().unwrap()))
            .collect::<Vec<_>>(),
    )
}
