use zksync_types::{witness_block_state::WitnessStorageState, StorageKey, StorageValue, H256};

use super::ReadStorage;

impl ReadStorage for WitnessStorageState {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        *self.read_storage_key.get(key).unwrap()
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        *self.is_write_initial.get(key).unwrap()
    }

    fn load_factory_dep(&mut self, _hash: H256) -> Option<Vec<u8>> {
        unreachable!("Factory deps should not be used in the witness storage")
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        unreachable!("Enumeration index should not be used in the witness storage")
    }
}
