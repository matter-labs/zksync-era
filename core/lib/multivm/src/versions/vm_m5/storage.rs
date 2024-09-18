use std::{cell::RefCell, collections::HashMap, fmt::Debug, rc::Rc};

use zksync_types::{StorageKey, StorageValue, H256};

use crate::interface::storage::{ReadStorage, WriteStorage};

pub trait Storage: Debug {
    fn get_value(&mut self, key: &StorageKey) -> StorageValue;
    // Returns the original value.
    fn set_value(&mut self, key: &StorageKey, value: StorageValue) -> StorageValue;
    fn is_write_initial(&mut self, key: &StorageKey) -> bool;
    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>>;

    fn number_of_updated_storage_slots(&self) -> usize;

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue>;
}

impl<T: WriteStorage> Storage for T {
    fn get_value(&mut self, key: &StorageKey) -> StorageValue {
        ReadStorage::read_value(self, key)
    }

    /// Returns the original value.
    fn set_value(&mut self, key: &StorageKey, value: StorageValue) -> StorageValue {
        WriteStorage::set_value(self, *key, value)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        ReadStorage::is_write_initial(self, key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        ReadStorage::load_factory_dep(self, hash)
    }

    fn number_of_updated_storage_slots(&self) -> usize {
        WriteStorage::modified_storage_keys(self).len()
    }

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        WriteStorage::modified_storage_keys(self)
    }
}

pub type StoragePtr<S> = Rc<RefCell<S>>;
