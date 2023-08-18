use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use zksync_state::{ReadStorage, StorageView, WriteStorage};
use zksync_types::{StorageKey, StorageValue, H256};

pub trait Storage: Debug + Sync + Send {
    fn get_value(&mut self, key: &StorageKey) -> StorageValue;
    // Returns the original value.
    fn set_value(&mut self, key: &StorageKey, value: StorageValue) -> StorageValue;
    fn is_write_initial(&mut self, key: &StorageKey) -> bool;
    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>>;

    fn number_of_updated_storage_slots(&self) -> usize;

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue>;
}

impl<S: ReadStorage + Debug + Send + Sync> Storage for StorageView<S> {
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
        self.get_modified_storage_keys().len()
    }

    fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        WriteStorage::modified_storage_keys(self)
    }
}

pub type StoragePtr<'a> = Rc<RefCell<&'a mut dyn Storage>>;
