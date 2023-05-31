use std::collections::HashMap;
use zksync_types::{
    get_code_key, get_system_context_init_logs, storage::ZkSyncReadStorage,
    system_contracts::get_system_smart_contracts, L2ChainId, StorageKey, StorageLog,
    StorageLogKind, StorageValue, H256,
};
use zksync_utils::bytecode::hash_bytecode;

/// An in-memory storage that contains the system contracts by default.
#[derive(Debug)]
pub struct InMemoryStorage {
    data: HashMap<StorageKey, StorageValue>,
    deps: HashMap<H256, Vec<u8>>,
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        let contracts = get_system_smart_contracts();
        let system_context_init_log = get_system_context_init_logs(L2ChainId(270));

        let mut data = HashMap::new();
        for log in contracts
            .iter()
            .map(|contract| {
                let deployer_code_key = get_code_key(contract.account_id.address());
                StorageLog::new_write_log(deployer_code_key, hash_bytecode(&contract.bytecode))
            })
            .chain(system_context_init_log)
        {
            if log.kind == StorageLogKind::Write {
                data.insert(log.key, log.value);
            }
        }

        let mut deps = HashMap::new();

        for contract in contracts {
            deps.insert(hash_bytecode(&contract.bytecode), contract.bytecode);
        }

        Self { data, deps }
    }
}

impl InMemoryStorage {
    pub fn set_value(&mut self, key: StorageKey, value: StorageValue) {
        self.data.insert(key, value);
    }
}

impl ZkSyncReadStorage for &InMemoryStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.data.get(key).cloned().unwrap_or(H256::zero())
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.data.contains_key(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.deps.get(&hash).cloned()
    }
}
