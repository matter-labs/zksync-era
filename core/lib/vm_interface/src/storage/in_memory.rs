use std::collections::{hash_map::Entry, BTreeMap, HashMap};

use zksync_types::{
    block::DeployedContract, bytecode::BytecodeHash, get_code_key, get_known_code_key,
    get_system_context_init_logs, system_contracts::get_system_smart_contracts, L2ChainId,
    StorageKey, StorageLog, StorageValue, H256,
};

use super::ReadStorage;

/// Network ID we use by default for in memory storage.
pub const IN_MEMORY_STORAGE_DEFAULT_NETWORK_ID: u32 = 270;

/// In-memory storage.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct InMemoryStorage {
    state: HashMap<H256, (StorageValue, u64)>,
    factory_deps: HashMap<H256, Vec<u8>>,
    last_enum_index_set: u64,
}

impl InMemoryStorage {
    /// Constructs a storage that contains system smart contracts.
    pub fn with_system_contracts() -> Self {
        Self::with_system_contracts_and_chain_id(L2ChainId::from(
            IN_MEMORY_STORAGE_DEFAULT_NETWORK_ID,
        ))
    }

    /// Constructs a storage that contains system smart contracts (with a given chain id).
    pub fn with_system_contracts_and_chain_id(chain_id: L2ChainId) -> Self {
        Self::with_custom_system_contracts_and_chain_id(chain_id, get_system_smart_contracts())
    }

    /// Constructs a storage that contains custom system contracts (provided in a vector).
    pub fn with_custom_system_contracts_and_chain_id(
        chain_id: L2ChainId,
        contracts: Vec<DeployedContract>,
    ) -> Self {
        let system_context_init_log = get_system_context_init_logs(chain_id);

        let state_without_indices: BTreeMap<_, _> = contracts
            .iter()
            .flat_map(|contract| {
                let bytecode_hash = BytecodeHash::for_bytecode(&contract.bytecode).value();

                let deployer_code_key = get_code_key(contract.account_id.address());
                let is_known_code_key = get_known_code_key(&bytecode_hash);

                [
                    StorageLog::new_write_log(deployer_code_key, bytecode_hash),
                    StorageLog::new_write_log(is_known_code_key, H256::from_low_u64_be(1)),
                ]
            })
            .chain(system_context_init_log)
            .filter_map(|log| (log.is_write()).then_some((log.key, log.value)))
            .collect();
        let state: HashMap<_, _> = state_without_indices
            .into_iter()
            .enumerate()
            .map(|(idx, (key, value))| (key.hashed_key(), (value, idx as u64 + 1)))
            .collect();

        let factory_deps = contracts
            .into_iter()
            .map(|contract| {
                (
                    BytecodeHash::for_bytecode(&contract.bytecode).value(),
                    contract.bytecode,
                )
            })
            .collect();

        let last_enum_index_set = state.len() as u64;
        Self {
            state,
            factory_deps,
            last_enum_index_set,
        }
    }

    /// Sets the storage `value` at the specified `key`.
    pub fn set_value_hashed_enum(&mut self, key: H256, enum_index_set: u64, value: StorageValue) {
        match self.state.entry(key) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().0 = value;
            }
            Entry::Vacant(entry) => {
                entry.insert((value, enum_index_set));
            }
        }
    }

    /// Sets the storage `value` at the specified `key`.
    pub fn set_value(&mut self, key: StorageKey, value: StorageValue) {
        match self.state.entry(key.hashed_key()) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().0 = value;
            }
            Entry::Vacant(entry) => {
                self.last_enum_index_set += 1;
                entry.insert((value, self.last_enum_index_set));
            }
        }
    }

    /// Stores a factory dependency with the specified `hash` and `bytecode`.
    pub fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
        self.factory_deps.insert(hash, bytecode);
    }
}

impl ReadStorage for &InMemoryStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.state
            .get(&key.hashed_key())
            .map(|(value, _)| value)
            .copied()
            .unwrap_or_default()
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        !self.state.contains_key(&key.hashed_key())
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.factory_deps.get(&hash).cloned()
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.state.get(&key.hashed_key()).map(|(_, idx)| *idx)
    }
}

impl ReadStorage for InMemoryStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        (&*self).read_value(key)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        (&*self).is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        (&*self).load_factory_dep(hash)
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        (&*self).get_enumeration_index(key)
    }
}
