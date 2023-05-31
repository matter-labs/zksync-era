use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use zksync_state::storage_view::StorageView;
use zksync_types::{get_known_code_key, StorageKey, StorageValue, ZkSyncReadStorage, H256};

pub trait Storage: Debug + Sync + Send {
    /// Returns a value from a given key. If value never existed, returns 0.
    fn get_value(&mut self, key: &StorageKey) -> StorageValue;
    // Sets the new value under a given key - returns the original value.
    fn set_value(&mut self, key: &StorageKey, value: StorageValue) -> StorageValue;
    /// The function returns true if it's the first time writing to this storage slot.
    /// The initial write uses 64 gas, while subsequent writes use only 40.
    fn is_write_initial(&mut self, key: &StorageKey) -> bool;
    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>>;

    fn number_of_updated_storage_slots(&self) -> usize {
        self.get_modified_storage_keys().len()
    }

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue>;

    /// Returns whether a bytecode hash is "known", i.e. whether
    /// it has been published on L1
    fn is_bytecode_known(&mut self, bytecode_hash: &H256) -> bool {
        let code_key = get_known_code_key(bytecode_hash);
        self.get_value(&code_key) != H256::zero()
    }

    fn missed_storage_invocations(&self) -> usize;
}

impl<S: ZkSyncReadStorage + Debug + Send + Sync> Storage for StorageView<S> {
    fn get_value(&mut self, key: &StorageKey) -> StorageValue {
        self.get_value(key)
    }

    /// Returns the original value.
    fn set_value(&mut self, key: &StorageKey, value: StorageValue) -> StorageValue {
        self.set_value(key, value)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.load_factory_dep(hash)
    }

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        self.get_modified_storage_keys()
    }

    fn missed_storage_invocations(&self) -> usize {
        self.storage_invocations_missed
    }
}

pub type StoragePtr<'a> = Rc<RefCell<&'a mut dyn Storage>>;
