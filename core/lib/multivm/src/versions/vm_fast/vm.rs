use std::collections::HashMap;

use vm2::{decode::decode_program, ExecutionEnd, Program, Settings, VirtualMachine};
use zk_evm_1_5_0::zkevm_opcode_defs::system_params::INITIAL_FRAME_FORMAL_EH_LOCATION;
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{AccountTreeId, StorageKey, BOOTLOADER_ADDRESS, H160, U256};

use crate::{interface::VmInterface, vm_latest::constants::VM_HOOK_POSITION, HistoryMode};

struct Vm {
    inner: VirtualMachine,
    suspended_at: u16,
}

impl Vm {
    fn run(&mut self) {
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
            0 => {}  // AccountValidationEntered,
            1 => {}  // PaymasterValidationEntered,
            2 => {}  // NoValidationEntered,
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

impl<S: WriteStorage + 'static, H: HistoryMode> VmInterface<S, H> for Vm {
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

        let mut inner = VirtualMachine::new(
            Box::new(World::new(storage)),
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
        }
    }

    fn push_transaction(&mut self, tx: zksync_types::Transaction) {
        todo!()
    }

    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: crate::vm_latest::VmExecutionMode,
    ) -> crate::vm_latest::VmExecutionResultAndLogs {
        todo!()
    }

    fn get_bootloader_memory(&self) -> crate::vm_latest::BootloaderMemory {
        todo!()
    }

    fn get_last_tx_compressed_bytecodes(
        &self,
    ) -> Vec<zksync_utils::bytecode::CompressedBytecodeInfo> {
        todo!()
    }

    fn start_new_l2_block(&mut self, l2_block_env: crate::vm_latest::L2BlockEnv) {
        todo!()
    }

    fn get_current_execution_state(&self) -> crate::vm_latest::CurrentExecutionState {
        todo!()
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> (
        Result<(), crate::interface::BytecodeCompressionError>,
        crate::vm_latest::VmExecutionResultAndLogs,
    ) {
        todo!()
    }

    fn record_vm_memory_metrics(&self) -> crate::vm_latest::VmMemoryMetrics {
        todo!()
    }

    fn gas_remaining(&self) -> u32 {
        todo!()
    }
}

struct World<S: WriteStorage> {
    storage: StoragePtr<S>,

    // TODO: It would be nice to store an LRU cache elsewhere.
    // This one is cleared on change of batch unfortunately.
    program_cache: HashMap<zksync_types::U256, Program>,
}

impl<S: WriteStorage> World<S> {
    fn new(storage: StoragePtr<S>) -> Self {
        Self {
            storage,
            program_cache: HashMap::new(),
        }
    }
}

impl<S: WriteStorage> vm2::World for World<S> {
    fn decommit(&mut self, hash: U256) -> Program {
        self.program_cache
            .entry(hash)
            .or_insert_with(|| {
                let mut hash_bytes = [0; 32];
                hash.to_big_endian(&mut hash_bytes);

                let bytecode = self
                    .storage
                    .borrow_mut()
                    .load_factory_dep(hash_bytes.into())
                    .expect("vm tried to decommit nonexistent bytecode");

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
            })
            .clone()
    }

    fn read_storage(
        &mut self,
        contract: zksync_types::H160,
        key: zksync_types::U256,
    ) -> zksync_types::U256 {
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
