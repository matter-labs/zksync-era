use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use zksync_state::storage_view::StorageView;
use zksync_types::{Address, StorageKey, StorageValue, ZkSyncReadStorage, H256};

pub trait Storage: Debug + Sync + Send {
    fn get_value(&mut self, key: &StorageKey) -> StorageValue;
    // Returns the original value.
    fn set_value(&mut self, key: &StorageKey, value: StorageValue) -> StorageValue;
    fn is_write_initial(&mut self, key: &StorageKey) -> bool;
    fn load_contract(&mut self, address: Address) -> Option<Vec<u8>>;
    fn save_contract(&mut self, address: Address, bytecode: Vec<u8>);
    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>>;
    fn save_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>);

    fn number_of_updated_storage_slots(&self) -> usize;

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue>;
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

    fn load_contract(&mut self, address: Address) -> Option<Vec<u8>> {
        self.load_contract(address)
    }

    fn save_contract(&mut self, address: Address, bytecode: Vec<u8>) {
        self.save_contract(address, bytecode);
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.load_factory_dep(hash)
    }

    fn save_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
        self.save_factory_dep(hash, bytecode);
    }

    fn number_of_updated_storage_slots(&self) -> usize {
        self.get_modified_storage_keys().len()
    }

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        self.get_modified_storage_keys()
    }
}

pub type StoragePtr<'a> = Rc<RefCell<&'a mut dyn Storage>>;
