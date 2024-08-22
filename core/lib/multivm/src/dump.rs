use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use zksync_types::{web3, L1BatchNumber, Transaction, H256};
use zksync_utils::u256_to_h256;
use zksync_vm_interface::{
    storage::{InMemoryStorage, ReadStorage, StoragePtr, StorageView},
    L1BatchEnv, L2BlockEnv, SystemEnv, VmInterface,
};

/// Handler for [`VmDump`].
pub type VmDumpHandler = Arc<dyn Fn(VmDump) + Send + Sync>;

/// Part of the VM dump representing the storage oracle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct VmStorageDump {
    pub(crate) read_storage_keys: HashMap<H256, (H256, u64)>,
    pub(crate) repeated_writes: HashMap<H256, u64>,
    pub(crate) factory_deps: HashMap<H256, web3::Bytes>,
}

impl VmStorageDump {
    /// Storage must be the one used by the VM.
    pub(crate) fn new<S: ReadStorage>(
        storage: &StoragePtr<StorageView<S>>,
        vm: &impl VmInterface,
    ) -> Self {
        let mut storage = storage.borrow_mut();
        let storage_cache = storage.cache();
        let read_storage_keys: HashMap<_, _> = storage_cache
            .read_storage_keys()
            .into_iter()
            .filter_map(|(key, value)| {
                let enum_index = storage.get_enumeration_index(&key)?;
                Some((key.hashed_key(), (value, enum_index)))
            })
            .collect();
        let repeated_writes = storage_cache
            .initial_writes()
            .into_iter()
            .filter_map(|(key, is_initial)| {
                let hashed_key = key.hashed_key();
                if !is_initial && !read_storage_keys.contains_key(&hashed_key) {
                    let enum_index = storage.get_enumeration_index(&key)?;
                    Some((hashed_key, enum_index))
                } else {
                    None
                }
            })
            .collect();

        // FIXME: may panic
        let used_contract_hashes = vm.get_current_execution_state().used_contract_hashes;
        let factory_deps = used_contract_hashes
            .into_iter()
            .filter_map(|hash| {
                let hash = u256_to_h256(hash);
                Some((hash, web3::Bytes(storage.load_factory_dep(hash)?)))
            })
            .collect();
        Self {
            read_storage_keys,
            repeated_writes,
            factory_deps,
        }
    }

    #[allow(dead_code)] // FIXME
    pub(crate) fn into_storage(self) -> InMemoryStorage {
        let mut storage = InMemoryStorage::default();
        for (key, (value, enum_index)) in self.read_storage_keys {
            storage.set_value_hashed_enum(key, enum_index, value);
        }
        for (key, enum_index) in self.repeated_writes {
            // The value shouldn't be read by the VM, so it doesn't matter.
            storage.set_value_hashed_enum(key, enum_index, H256::zero());
        }
        for (hash, bytecode) in self.factory_deps {
            storage.store_factory_dep(hash, bytecode.0);
        }
        storage
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "snake_case")]
pub(crate) enum VmAction {
    Block(L2BlockEnv),
    Transaction(Box<Transaction>),
}

/// VM dump allowing to re-run the VM on the same inputs. Opaque, but can be (de)serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct VmDump {
    pub(crate) l1_batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,
    pub(crate) actions: Vec<VmAction>,
    #[serde(flatten)]
    pub(crate) storage: VmStorageDump,
}

impl VmDump {
    pub(crate) fn new(l1_batch_env: L1BatchEnv, system_env: SystemEnv) -> Self {
        Self {
            l1_batch_env,
            system_env,
            actions: vec![],
            storage: VmStorageDump::default(),
        }
    }

    pub fn l1_batch_number(&self) -> L1BatchNumber {
        self.l1_batch_env.number
    }

    pub(crate) fn push_transaction(&mut self, tx: Transaction) {
        let tx = VmAction::Transaction(Box::new(tx));
        self.actions.push(tx);
    }

    pub(crate) fn push_block(&mut self, block: L2BlockEnv) {
        self.actions.push(VmAction::Block(block));
    }

    pub(crate) fn set_storage(&mut self, storage: VmStorageDump) {
        self.storage = storage;
    }
}
