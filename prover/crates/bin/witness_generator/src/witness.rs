use zksync_multivm::interface::storage::ReadStorage;
use zksync_types::{witness_block_state::WitnessStorageState, StorageKey, StorageValue, H256};

/// [`ReadStorage`] implementation backed by binary serialized [`WitnessHashBlockState`].
/// Note that `load_factory_deps` is not used.
/// FactoryDeps data is used straight inside witness generator, loaded with the blob.
#[derive(Debug)]
pub struct WitnessStorage {
    storage_state: WitnessStorageState,
}

impl WitnessStorage {
    /// Creates a new storage with the provided witness's block state.
    pub fn new(storage_state: WitnessStorageState) -> Self {
        Self { storage_state }
    }
}

impl ReadStorage for WitnessStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.storage_state
            .read_storage_key
            .get(key)
            .copied()
            .unwrap_or_default()
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.storage_state
            .is_write_initial
            .get(key)
            .copied()
            .unwrap_or_default()
    }

    fn load_factory_dep(&mut self, _hash: H256) -> Option<Vec<u8>> {
        unreachable!("Factory deps should not be used in the witness storage")
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        unreachable!("Enumeration index should not be used in the witness storage")
    }
}
