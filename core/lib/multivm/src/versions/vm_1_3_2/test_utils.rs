//!
//! This file contains various utilities
//! that could be used for testing, but are not needed anywhere else.
//!
//! They are not put into the `cfg(test)` folder to allow easy sharing of the content
//! of this file with other crates.
//!

use std::collections::HashMap;

use itertools::Itertools;
use zk_evm_1_3_3::{aux_structures::Timestamp, vm_state::VmLocalState};
use zksync_contracts::test_contracts::LoadnextContractExecutionParams;
use zksync_contracts::{deployer_contract, get_loadnext_contract, load_contract};
use zksync_state::WriteStorage;
use zksync_types::{
    ethabi::{Address, Token},
    fee::Fee,
    l2::L2Tx,
    web3::signing::keccak256,
    Execute, L2ChainId, Nonce, StorageKey, StorageLogQuery, StorageValue,
    CONTRACT_DEPLOYER_ADDRESS, H256, U256,
};
use zksync_utils::{
    address_to_h256, bytecode::hash_bytecode, h256_to_account_address, u256_to_h256,
};

/// The tests here help us with the testing the VM
use crate::vm_1_3_2::{
    event_sink::InMemoryEventSink,
    history_recorder::{
        AppDataFrameManagerWithHistory, HistoryEnabled, HistoryMode, HistoryRecorder,
    },
    memory::SimpleMemory,
    vm_instance::ZkSyncVmState,
    VmInstance,
};

#[derive(Clone, Debug)]
pub struct ModifiedKeysMap(HashMap<StorageKey, StorageValue>);

// We consider hashmaps to be equal even if there is a key
// that is not present in one but has zero value in another.
impl PartialEq for ModifiedKeysMap {
    fn eq(&self, other: &Self) -> bool {
        for (key, value) in self.0.iter() {
            if *value != other.0.get(key).cloned().unwrap_or_default() {
                return false;
            }
        }
        for (key, value) in other.0.iter() {
            if *value != self.0.get(key).cloned().unwrap_or_default() {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct DecommitterTestInnerState<H: HistoryMode> {
    /// There is no way to "trully" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub modified_storage_keys: ModifiedKeysMap,
    pub known_bytecodes: HistoryRecorder<HashMap<U256, Vec<U256>>, H>,
    pub decommitted_code_hashes: HistoryRecorder<HashMap<U256, u32>, HistoryEnabled>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct StorageOracleInnerState<H: HistoryMode> {
    /// There is no way to "trully" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub modified_storage_keys: ModifiedKeysMap,

    pub frames_stack: AppDataFrameManagerWithHistory<Box<StorageLogQuery>, H>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct PrecompileProcessorTestInnerState<H: HistoryMode> {
    pub timestamp_history: HistoryRecorder<Vec<Timestamp>, H>,
}

/// A struct that encapsulates the state of the VM's oracles
/// The state is to be used in tests.
#[derive(Clone, PartialEq, Debug)]
pub struct VmInstanceInnerState<H: HistoryMode> {
    event_sink: InMemoryEventSink<H>,
    precompile_processor_state: PrecompileProcessorTestInnerState<H>,
    memory: SimpleMemory<H>,
    decommitter_state: DecommitterTestInnerState<H>,
    storage_oracle_state: StorageOracleInnerState<H>,
    local_state: VmLocalState,
}

impl<H: HistoryMode, S: WriteStorage> VmInstance<S, H> {
    /// This method is mostly to be used in tests. It dumps the inner state of all the oracles and the VM itself.
    pub fn dump_inner_state(&self) -> VmInstanceInnerState<H> {
        let event_sink = self.state.event_sink.clone();
        let precompile_processor_state = PrecompileProcessorTestInnerState {
            timestamp_history: self.state.precompiles_processor.timestamp_history.clone(),
        };
        let memory = self.state.memory.clone();
        let decommitter_state = DecommitterTestInnerState {
            modified_storage_keys: ModifiedKeysMap(
                self.state
                    .decommittment_processor
                    .get_storage()
                    .borrow()
                    .modified_storage_keys()
                    .clone(),
            ),
            known_bytecodes: self.state.decommittment_processor.known_bytecodes.clone(),
            decommitted_code_hashes: self
                .state
                .decommittment_processor
                .get_decommitted_code_hashes_with_history()
                .clone(),
        };
        let storage_oracle_state = StorageOracleInnerState {
            modified_storage_keys: ModifiedKeysMap(
                self.state
                    .storage
                    .storage
                    .get_ptr()
                    .borrow()
                    .modified_storage_keys()
                    .clone(),
            ),
            frames_stack: self.state.storage.frames_stack.clone(),
        };
        let local_state = self.state.local_state.clone();

        VmInstanceInnerState {
            event_sink,
            precompile_processor_state,
            memory,
            decommitter_state,
            storage_oracle_state,
            local_state,
        }
    }
}

// This one is used only for tests, but it is in this folder to
// be able to share it among crates
pub fn mock_loadnext_test_call(
    eth_private_key: H256,
    nonce: Nonce,
    contract_address: Address,
    fee: Fee,
    execution_params: LoadnextContractExecutionParams,
) -> L2Tx {
    let loadnext_contract = get_loadnext_contract();

    let contract_function = loadnext_contract.contract.function("execute").unwrap();

    let params = vec![
        Token::Uint(U256::from(execution_params.reads)),
        Token::Uint(U256::from(execution_params.writes)),
        Token::Uint(U256::from(execution_params.hashes)),
        Token::Uint(U256::from(execution_params.events)),
        Token::Uint(U256::from(execution_params.recursive_calls)),
        Token::Uint(U256::from(execution_params.deploys)),
    ];
    let calldata = contract_function
        .encode_input(&params)
        .expect("failed to encode parameters");

    let mut l2_tx = L2Tx::new_signed(
        contract_address,
        calldata,
        nonce,
        fee,
        Default::default(),
        L2ChainId::from(270),
        &eth_private_key,
        None,
        Default::default(),
    )
    .unwrap();
    // Input means all transaction data (NOT calldata, but all tx fields) that came from the API.
    // This input will be used for the derivation of the tx hash, so put some random to it to be sure
    // that the transaction hash is unique.
    l2_tx.set_input(H256::random().0.to_vec(), H256::random());
    l2_tx
}

// This one is used only for tests, but it is in this folder to
// be able to share it among crates
pub fn mock_loadnext_gas_burn_call(
    eth_private_key: H256,
    nonce: Nonce,
    contract_address: Address,
    fee: Fee,
    gas: u32,
) -> L2Tx {
    let loadnext_contract = get_loadnext_contract();

    let contract_function = loadnext_contract.contract.function("burnGas").unwrap();

    let params = vec![Token::Uint(U256::from(gas))];
    let calldata = contract_function
        .encode_input(&params)
        .expect("failed to encode parameters");

    let mut l2_tx = L2Tx::new_signed(
        contract_address,
        calldata,
        nonce,
        fee,
        Default::default(),
        L2ChainId::from(270),
        &eth_private_key,
        None,
        Default::default(),
    )
    .unwrap();
    // Input means all transaction data (NOT calldata, but all tx fields) that came from the API.
    // This input will be used for the derivation of the tx hash, so put some random to it to be sure
    // that the transaction hash is unique.
    l2_tx.set_input(H256::random().0.to_vec(), H256::random());
    l2_tx
}

pub fn get_create_execute(code: &[u8], calldata: &[u8]) -> Execute {
    let deployer = deployer_contract();

    let contract_function = deployer.function("create").unwrap();

    let params = [
        Token::FixedBytes(vec![0u8; 32]),
        Token::FixedBytes(hash_bytecode(code).0.to_vec()),
        Token::Bytes(calldata.to_vec()),
    ];
    let calldata = contract_function
        .encode_input(&params)
        .expect("failed to encode parameters");

    Execute {
        contract_address: CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        factory_deps: Some(vec![code.to_vec()]),
        value: U256::zero(),
    }
}

fn get_execute_error_calldata() -> Vec<u8> {
    let test_contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/error/error.sol/SimpleRequire.json",
    );

    let function = test_contract.function("require_short").unwrap();

    function
        .encode_input(&[])
        .expect("failed to encode parameters")
}

pub fn get_deploy_tx(
    account_private_key: H256,
    nonce: Nonce,
    code: &[u8],
    factory_deps: Vec<Vec<u8>>,
    calldata: &[u8],
    fee: Fee,
) -> L2Tx {
    let factory_deps = factory_deps
        .into_iter()
        .chain(vec![code.to_vec()])
        .collect();
    let execute = get_create_execute(code, calldata);

    let mut signed = L2Tx::new_signed(
        CONTRACT_DEPLOYER_ADDRESS,
        execute.calldata,
        nonce,
        fee,
        U256::zero(),
        L2ChainId::from(270),
        &account_private_key,
        Some(factory_deps),
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());

    signed
}

pub fn get_error_tx(
    account_private_key: H256,
    nonce: Nonce,
    contract_address: Address,
    fee: Fee,
) -> L2Tx {
    let factory_deps = vec![];
    let calldata = get_execute_error_calldata();

    let mut signed = L2Tx::new_signed(
        contract_address,
        calldata,
        nonce,
        fee,
        U256::zero(),
        L2ChainId::from(270),
        &account_private_key,
        Some(factory_deps),
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());

    signed
}

pub fn get_create_zksync_address(sender_address: Address, sender_nonce: Nonce) -> Address {
    let prefix = keccak256("zksyncCreate".as_bytes());
    let address = address_to_h256(&sender_address);
    let nonce = u256_to_h256(U256::from(sender_nonce.0));

    let digest = prefix
        .iter()
        .chain(address.0.iter())
        .chain(nonce.0.iter())
        .copied()
        .collect_vec();

    let hash = keccak256(&digest);

    h256_to_account_address(&H256(hash))
}

pub fn verify_required_storage<H: HistoryMode, S: WriteStorage>(
    state: &ZkSyncVmState<S, H>,
    required_values: Vec<(H256, StorageKey)>,
) {
    for (required_value, key) in required_values {
        let current_value = state.storage.storage.read_from_storage(&key);

        assert_eq!(
            u256_to_h256(current_value),
            required_value,
            "Invalid value at key {key:?}"
        );
    }
}
