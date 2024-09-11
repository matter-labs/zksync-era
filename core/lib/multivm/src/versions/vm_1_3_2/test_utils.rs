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
use zksync_contracts::deployer_contract;
use zksync_types::{
    ethabi::{Address, Token},
    web3::keccak256,
    Execute, Nonce, StorageKey, StorageValue, CONTRACT_DEPLOYER_ADDRESS, H256, U256,
};
use zksync_utils::{
    address_to_h256, bytecode::hash_bytecode, h256_to_account_address, u256_to_h256,
};

use crate::interface::storage::WriteStorage;
/// The tests here help us with the testing the VM
use crate::vm_1_3_2::{
    event_sink::InMemoryEventSink,
    history_recorder::{
        AppDataFrameManagerWithHistory, HistoryEnabled, HistoryMode, HistoryRecorder,
    },
    memory::SimpleMemory,
    utils::StorageLogQuery,
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
    /// There is no way to "truly" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub modified_storage_keys: ModifiedKeysMap,
    pub known_bytecodes: HistoryRecorder<HashMap<U256, Vec<U256>>, H>,
    pub decommitted_code_hashes: HistoryRecorder<HashMap<U256, u32>, HistoryEnabled>,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct StorageOracleInnerState<H: HistoryMode> {
    /// There is no way to "truly" compare the storage pointer,
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
        contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
        calldata,
        factory_deps: vec![code.to_vec()],
        value: U256::zero(),
    }
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
