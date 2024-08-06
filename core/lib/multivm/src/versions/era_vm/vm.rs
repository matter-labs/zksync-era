use std::{cell::RefCell, collections::HashMap, fmt::Write, io::Read, rc::Rc, str::FromStr};

use era_vm::{
    store::{InMemory, L2ToL1Log, StorageError, StorageKey as EraStorageKey},
    vm::ExecutionOutput,
    LambdaVm, VMState,
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    l1::is_l1_tx_type, AccountTreeId, StorageKey, Transaction, BOOTLOADER_ADDRESS, H160,
    KNOWN_CODES_STORAGE_ADDRESS, U256,
};
use zksync_utils::{
    bytecode::{hash_bytecode, CompressedBytecodeInfo},
    h256_to_u256, u256_to_h256,
};

use super::{
    bootloader_state::{utils::apply_l2_block, BootloaderState},
    event::merge_events,
    hook::Hook,
    initial_bootloader_memory::bootloader_initial_memory,
    snapshot::VmSnapshot,
};
use crate::{
    era_vm::{bytecode::compress_bytecodes, transaction_data::TransactionData},
    interface::{Halt, TxRevertReason, VmInterface, VmInterfaceHistoryEnabled, VmRevertReason},
    vm_latest::{
        constants::{
            get_vm_hook_position, get_vm_hook_start_position_latest, VM_HOOK_PARAMS_COUNT,
        },
        BootloaderMemory, CurrentExecutionState, ExecutionResult, HistoryEnabled, L1BatchEnv,
        L2BlockEnv, SystemEnv, VmExecutionLogs, VmExecutionMode, VmExecutionResultAndLogs,
    },
};

pub struct Vm<S: WriteStorage> {
    pub(crate) inner: LambdaVm,
    suspended_at: u16,
    gas_for_account_validation: u32,
    last_tx_result: Option<ExecutionResult>,

    bootloader_state: BootloaderState,
    pub(crate) storage: StoragePtr<S>,

    // TODO: Maybe not necessary, check
    program_cache: Rc<RefCell<HashMap<U256, Vec<U256>>>>,

    // these two are only needed for tests so far
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,

    snapshots: Vec<VmSnapshot>, // TODO: Implement snapshots logic
}

impl<S: WriteStorage + 'static> Vm<S> {
    pub fn run(&mut self, execution_mode: VmExecutionMode) -> (ExecutionResult, VMState) {
        loop {
            let (result, mut final_vm) = self.inner.run_program_with_custom_bytecode();
            let result = match result {
                ExecutionOutput::Ok(output) => {
                    return (ExecutionResult::Success { output }, final_vm)
                }
                ExecutionOutput::Revert(output) => match TxRevertReason::parse_error(&output) {
                    TxRevertReason::TxReverted(output) => {
                        return (ExecutionResult::Revert { output }, final_vm)
                    }
                    TxRevertReason::Halt(reason) => {
                        return (ExecutionResult::Halt { reason }, final_vm)
                    }
                },
                ExecutionOutput::Panic => {
                    return (
                        ExecutionResult::Halt {
                            reason: if self.inner.state.gas_left().unwrap() == 0 {
                                Halt::BootloaderOutOfGas
                            } else {
                                Halt::VMPanic
                            },
                        },
                        final_vm,
                    )
                }
                ExecutionOutput::SuspendedOnHook {
                    hook,
                    pc_to_resume_from,
                } => {
                    self.suspended_at = pc_to_resume_from;
                    hook
                }
            };

            match Hook::from_u32(result) {
                Hook::FinalBatchInfo => {
                    // println!("FINAL BATCH INFO");
                    // set fictive l2 block
                    let txs_index = self.bootloader_state.free_tx_index();
                    let l2_block = self.bootloader_state.insert_fictive_l2_block();
                    let mut memory = vec![];
                    apply_l2_block(&mut memory, l2_block, txs_index);
                    self.write_to_bootloader_heap(memory);
                }
                Hook::AccountValidationEntered => {
                    // println!("ACCOUNT VALIDATION ENTERED");
                }
                Hook::ValidationStepEnded => {
                    // println!("VALIDATION STEP ENDED");
                }
                Hook::AccountValidationExited => {
                    // println!("ACCOUNT VALIDATION EXITED");
                }
                Hook::DebugReturnData => {
                    // println!("DEBUG RETURN DATA");
                }
                Hook::PostResult => {
                    // println!("POST RESULT");
                    let result = self.get_hook_params()[0];
                    // println!("RESULT: {:?}", result);

                    // TODO get latest return data
                    let return_data = vec![];

                    self.last_tx_result = Some(if result.is_zero() {
                        // println!("Reverted");
                        ExecutionResult::Revert {
                            output: VmRevertReason::from(return_data.as_slice()),
                        }
                    } else {
                        ExecutionResult::Success {
                            output: return_data,
                        }
                    });
                }
                Hook::NotifyAboutRefund => {
                    // println!("NOTIFY ABOUT REFUND");
                }
                Hook::AskOperatorForRefund => {
                    // println!("ASK OPERATOR FOR REFUND");
                }
                Hook::DebugLog => {
                    // println!("DEBUG LOG");
                }
                Hook::TxHasEnded => {
                    // println!("TX HAS ENDED");
                    if let VmExecutionMode::OneTx = execution_mode {
                        return (self.last_tx_result.take().unwrap(), final_vm);
                    }
                }
            }
            final_vm.current_frame_mut().unwrap().pc = self.suspended_at as u64;
            self.inner.state = final_vm;
        }
    }

    pub(crate) fn insert_bytecodes<'a>(&mut self, bytecodes: impl IntoIterator<Item = &'a [u8]>) {
        for code in bytecodes {
            let mut program_code = vec![];
            for raw_opcode_slice in code.chunks(32) {
                let mut raw_opcode_bytes: [u8; 32] = [0; 32];
                raw_opcode_bytes.copy_from_slice(&raw_opcode_slice[..32]);
                let raw_opcode_u256 = U256::from_big_endian(&raw_opcode_bytes);
                program_code.push(raw_opcode_u256);
            }
            // println!("HASH: {:?}", U256::from_big_endian(hash_bytecode(code).as_bytes()));
            // println!("PROGRAM CODE: {:?}", program_code);
            self.program_cache.borrow_mut().insert(
                U256::from_big_endian(hash_bytecode(code).as_bytes()),
                program_code,
            );
        }
    }

    fn get_hook_params(&self) -> [U256; 3] {
        let vm_hooks_param_start = get_vm_hook_start_position_latest();
        (vm_hooks_param_start..vm_hooks_param_start + VM_HOOK_PARAMS_COUNT)
            .map(|word| {
                let res = self.read_heap_word(word as usize);
                // println!("WORD: {:?} RES: {:?}", word, res);
                res
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
        // [U256::from_dec_str("29723826").unwrap() ,U256::zero() ,U256::from_dec_str("3400").unwrap()]
    }

    /// Typically used to read the bootloader heap. We know that we're in the bootloader
    /// when a hook occurs, as they are only enabled when preprocessing bootloader code.
    fn read_heap_word(&self, word: usize) -> U256 {
        let heap = self
            .inner
            .state
            .heaps
            .get(self.inner.state.current_context().unwrap().heap_id)
            .unwrap();
        heap.read((word * 32) as u32)
    }

    #[cfg(test)]
    /// Returns the current state of the VM in a format that can be compared for equality.
    pub(crate) fn dump_state(&self) -> VMState {
        self.inner.state.clone()
    }

    fn write_to_bootloader_heap(&mut self, memory: impl IntoIterator<Item = (usize, U256)>) {
        assert!(self.inner.state.running_contexts.len() == 1); // No on-going far calls
        if let Some(heap) = &mut self
            .inner
            .state
            .heaps
            .get_mut(self.inner.state.current_context().unwrap().heap_id)
        {
            for (slot, value) in memory {
                let end = (slot + 1) * 32;
                heap.expand_memory(end as u32);
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
        let vm_hook_position =
            get_vm_hook_position(crate::vm_latest::MultiVMSubversion::IncreasedBootloaderMemory)
                * 32;
        let vm_state = VMState::new(
            bootloader_code.to_owned(),
            Vec::new(),
            BOOTLOADER_ADDRESS,
            H160::zero(),
            0_u128,
            system_env
                .base_system_smart_contracts
                .default_aa
                .hash
                .to_fixed_bytes(),
            vm_hook_position,
        );
        let pre_contract_storage = Rc::new(RefCell::new(HashMap::new()));
        pre_contract_storage.borrow_mut().insert(
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
        let world_storage = World::new(storage.clone(), pre_contract_storage.clone());
        let mut vm = LambdaVm::new(vm_state, Rc::new(RefCell::new(world_storage)));
        let bootloader_memory = bootloader_initial_memory(&batch_env);

        // The bootloader shouldn't pay for growing memory and it writes results
        // to the end of its heap, so it makes sense to preallocate it in its entirety.
        const BOOTLOADER_MAX_MEMORY_SIZE: u32 = 59000000;
        vm.state
            .heaps
            .get_mut(era_vm::state::FIRST_HEAP)
            .unwrap()
            .expand_memory(BOOTLOADER_MAX_MEMORY_SIZE);
        vm.state
            .heaps
            .get_mut(era_vm::state::FIRST_HEAP + 1)
            .unwrap()
            .expand_memory(BOOTLOADER_MAX_MEMORY_SIZE);

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
            program_cache: pre_contract_storage,
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

        self.insert_bytecodes(tx.factory_deps.iter().map(|dep| &dep[..]));

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
        self.inner.state.current_frame().unwrap().gas_left.0
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

#[derive(Debug, Clone)]
pub struct World<S: WriteStorage> {
    pub storage: StoragePtr<S>,
    pub contract_storage: Rc<RefCell<HashMap<U256, Vec<U256>>>>,
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
}

impl<S: WriteStorage> World<S> {
    pub fn new_empty(storage: StoragePtr<S>) -> Self {
        let contract_storage = Rc::new(RefCell::new(HashMap::new()));
        let l2_to_l1_logs = Vec::new();
        Self {
            contract_storage,
            storage,
            l2_to_l1_logs,
        }
    }

    pub fn new(
        storage: StoragePtr<S>,
        contract_storage: Rc<RefCell<HashMap<U256, Vec<U256>>>>,
    ) -> Self {
        Self {
            storage,
            contract_storage,
            l2_to_l1_logs: Vec::new(),
        }
    }
}

impl<S: WriteStorage> era_vm::store::Storage for World<S> {
    fn decommit(&self, hash: U256) -> Result<Option<Vec<U256>>, StorageError> {
        Ok(Some(
            self.contract_storage
                .borrow_mut()
                .entry(hash)
                .or_insert_with(|| {
                    let contract = self
                        .storage
                        .borrow_mut()
                        .load_factory_dep(u256_to_h256(hash))
                        .expect("Bytecode not found");
                    let mut program_code = vec![];
                    for raw_opcode_slice in contract.chunks(32) {
                        let mut raw_opcode_bytes: [u8; 32] = [0; 32];
                        raw_opcode_bytes.copy_from_slice(&raw_opcode_slice[..32]);

                        let raw_opcode_u256 = U256::from_big_endian(&raw_opcode_bytes);
                        program_code.push(raw_opcode_u256);
                    }
                    program_code
                })
                .clone(),
        ))
    }

    fn add_contract(&mut self, hash: U256, code: Vec<U256>) -> Result<(), StorageError> {
        self.contract_storage.borrow_mut().insert(hash, code);
        Ok(())
    }

    fn storage_drop(&mut self, key: EraStorageKey) -> Result<(), StorageError> {
        println!("STORAGE DROP");
        todo!()
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

    fn record_l2_to_l1_log(&mut self, log: L2ToL1Log) -> Result<(), StorageError> {
        self.l2_to_l1_logs.push(log);
        Ok(())
    }

    fn fake_clone(&self) -> InMemory {
        println!("FAKE CLONE");
        // InMemory::new(self.contract_storage.clone(), self.storage.clone())
        InMemory::new_empty()
    }

    fn get_all_keys(&self) -> Vec<EraStorageKey> {
        println!("GET ALL KEYS");
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use once_cell::sync::Lazy;
    use zksync_contracts::{deployer_contract, BaseSystemContracts};
    use zksync_state::{InMemoryStorage, StorageView};
    use zksync_types::{
        block::L2BlockHasher,
        ethabi::{encode, Token},
        fee::Fee,
        fee_model::BatchFeeInput,
        helpers::unix_timestamp_ms,
        l2::L2Tx,
        utils::storage_key_for_eth_balance,
        Address, K256PrivateKey, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce, ProtocolVersionId,
        Transaction, CONTRACT_DEPLOYER_ADDRESS, H256, U256,
    };
    use zksync_utils::bytecode::hash_bytecode;

    use crate::{
        era_vm::vm::Vm,
        interface::{
            L2BlockEnv, TxExecutionMode, VmExecutionMode, VmExecutionResultAndLogs, VmInterface,
        },
        utils::get_max_gas_per_pubdata_byte,
        vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
    };
    /// Bytecodes have consist of an odd number of 32 byte words
    /// This function "fixes" bytecodes of wrong length by cutting off their end.
    pub fn cut_to_allowed_bytecode_size(bytes: &[u8]) -> Option<&[u8]> {
        let mut words = bytes.len() / 32;
        if words == 0 {
            return None;
        }
        if words & 1 == 0 {
            words -= 1;
        }
        Some(&bytes[..32 * words])
    }

    static PRIVATE_KEY: Lazy<K256PrivateKey> =
        Lazy::new(|| K256PrivateKey::from_bytes(H256([42; 32])).expect("invalid key bytes"));
    static SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
        Lazy::new(BaseSystemContracts::load_from_disk);
    static STORAGE: Lazy<InMemoryStorage> = Lazy::new(|| {
        let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

        // Give `PRIVATE_KEY` some money
        let key = storage_key_for_eth_balance(&PRIVATE_KEY.address());
        storage.set_value(key, zksync_utils::u256_to_h256(U256([0, 0, 1, 0])));

        storage
    });
    static CREATE_FUNCTION_SIGNATURE: Lazy<[u8; 4]> = Lazy::new(|| {
        deployer_contract()
            .function("create")
            .unwrap()
            .short_signature()
    });

    pub fn get_deploy_tx(code: &[u8]) -> Transaction {
        let params = [
            Token::FixedBytes(vec![0u8; 32]),
            Token::FixedBytes(hash_bytecode(code).0.to_vec()),
            Token::Bytes([].to_vec()),
        ];
        let calldata = CREATE_FUNCTION_SIGNATURE
            .iter()
            .cloned()
            .chain(encode(&params))
            .collect();

        let mut signed = L2Tx::new_signed(
            CONTRACT_DEPLOYER_ADDRESS,
            calldata,
            Nonce(0),
            Fee {
                gas_limit: U256::from(30000000u32),
                max_fee_per_gas: U256::from(250_000_000),
                max_priority_fee_per_gas: U256::from(0),
                gas_per_pubdata_limit: U256::from(get_max_gas_per_pubdata_byte(
                    ProtocolVersionId::latest().into(),
                )),
            },
            U256::zero(),
            L2ChainId::from(270),
            &PRIVATE_KEY,
            vec![code.to_vec()], // maybe not needed?
            Default::default(),
        )
        .expect("should create a signed execute transaction");

        signed.set_input(H256::random().as_bytes().to_vec(), H256::random());

        signed.into()
    }

    #[test]
    fn test_vm() {
        let path = PathBuf::from("./src/versions/era_vm/test_contract/storage");
        let test_contract = std::fs::read(&path).expect("failed to read file");
        let code = cut_to_allowed_bytecode_size(&test_contract).unwrap();
        let tx = get_deploy_tx(code);
        let timestamp = unix_timestamp_ms();
        let mut vm = Vm::new(
            crate::interface::L1BatchEnv {
                previous_batch_hash: None,
                number: L1BatchNumber(1),
                timestamp,
                fee_input: BatchFeeInput::l1_pegged(
                    50_000_000_000, // 50 gwei
                    250_000_000,    // 0.25 gwei
                ),
                fee_account: Address::random(),
                enforced_base_fee: None,
                first_l2_block: L2BlockEnv {
                    number: 1,
                    timestamp,
                    prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
                    max_virtual_blocks_to_create: 100,
                },
            },
            crate::interface::SystemEnv {
                zk_porter_available: false,
                version: ProtocolVersionId::latest(),
                base_system_smart_contracts: SYSTEM_CONTRACTS.clone(),
                bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                chain_id: L2ChainId::from(270),
            },
            Rc::new(RefCell::new(StorageView::new(&*STORAGE))),
        );
        vm.push_transaction(tx);
        let a = vm.execute(VmExecutionMode::OneTx);
        println!("{:?}", a.result);
    }
}
