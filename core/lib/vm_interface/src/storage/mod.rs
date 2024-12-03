use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use zksync_types::{get_known_code_key, StorageKey, StorageValue, H256};

pub use self::{
    // Note, that `test_infra` of the bootloader tests relies on this value to be exposed
    in_memory::{InMemoryStorage, IN_MEMORY_STORAGE_DEFAULT_NETWORK_ID},
    overrides::StorageWithOverrides,
    snapshot::{StorageSnapshot, StorageWithSnapshot},
    view::{ImmutableStorageView, StorageView, StorageViewCache, StorageViewStats},
};

mod in_memory;
mod overrides;
mod snapshot;
mod view;

/// Functionality to read from the VM storage.
pub trait ReadStorage: fmt::Debug {
    /// Read value of the key.
    fn read_value(&mut self, key: &StorageKey) -> StorageValue;

    /// Checks whether a write to this storage at the specified `key` would be an initial write.
    /// Roughly speaking, this is the case when the storage doesn't contain `key`, although
    /// in case of mutable storage, the caveats apply (a write to a key that is present
    /// in the storage but was not committed is still an initial write).
    fn is_write_initial(&mut self, key: &StorageKey) -> bool;

    /// Load the factory dependency code by its hash.
    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>>;

    /// Returns whether a bytecode hash is "known" to the system.
    fn is_bytecode_known(&mut self, bytecode_hash: &H256) -> bool {
        let code_key = get_known_code_key(bytecode_hash);
        self.read_value(&code_key) != H256::zero()
    }

    /// Retrieves the enumeration index for a given `key`.
    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64>;
}

/// Functionality to write to the VM storage in a batch.
///
/// So far, this trait is implemented only for [`StorageView`].
pub trait WriteStorage: ReadStorage {
    /// Returns the map with the key–value pairs read by this batch.
    fn read_storage_keys(&self) -> &HashMap<StorageKey, StorageValue>;

    /// Sets the new value under a given key and returns the previous value.
    fn set_value(&mut self, key: StorageKey, value: StorageValue) -> StorageValue;

    /// Returns a map with the key–value pairs updated by this batch.
    fn modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue>;

    /// Returns the number of read / write ops for which the value was read from the underlying
    /// storage.
    fn missed_storage_invocations(&self) -> usize;
}

/// Smart pointer to [`WriteStorage`].
pub type StoragePtr<S> = Rc<RefCell<S>>;

impl<S: ReadStorage> ReadStorage for StoragePtr<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.borrow_mut().read_value(key)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.borrow_mut().is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.borrow_mut().load_factory_dep(hash)
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.borrow_mut().get_enumeration_index(key)
    }
}
