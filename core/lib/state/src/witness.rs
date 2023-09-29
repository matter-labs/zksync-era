use crate::ReadStorage;

use zksync_types::{witness_block_state::WitnessHashBlockState, StorageKey, StorageValue, H256};

/// [`ReadStorage`] implementation backed by binary serialized [`WitnessHashBlockState`].
/// Note that `load_factory_deps` is not used.
/// FactoryDeps data is used straight inside witness generator, loaded with the blob.
#[derive(Debug)]
pub struct WitnessStorage {
    block_state: WitnessHashBlockState,
}

impl WitnessStorage {
    /// Creates a new storage with the provided witness's block state.
    pub fn new(block_state: WitnessHashBlockState) -> Self {
        Self { block_state }
    }
}

impl ReadStorage for WitnessStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        *self
            .block_state
            .read_storage_key
            .get(&key.hashed_key_u256())
            .unwrap_or(&H256::default())
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        *self
            .block_state
            .is_write_initial
            .get(&key.hashed_key_u256())
            .unwrap_or(&false)
    }

    fn load_factory_dep(&mut self, _hash: H256) -> Option<Vec<u8>> {
        None
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        metrics::histogram!("witness_storage.unexpected_get_enumeration_index_call", 1.0);
        None
    }
}
