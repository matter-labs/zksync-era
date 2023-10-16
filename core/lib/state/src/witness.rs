use crate::ReadStorage;

use zksync_types::witness_block_state::WitnessBlockState;
use zksync_types::{witness_block_state::WitnessHashBlockState, StorageKey, StorageValue, H256};

/// [`ReadStorage`] implementation backed by binary serialized [`WitnessHashBlockState`].
/// Note that `load_factory_deps` is not used.
/// FactoryDeps data is used straight inside witness generator, loaded with the blob.
#[derive(Debug)]
pub struct WitnessStorage {
    block_state: WitnessBlockState,
}

impl WitnessStorage {
    /// Creates a new storage with the provided witness's block state.
    pub fn new(block_state: WitnessBlockState) -> Self {
        Self { block_state }
    }
}

impl ReadStorage for WitnessStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        *self.block_state.read_storage_key.get(&key).unwrap_or({
            // println!("giving default for read_value, wot? {key:?}");
            &H256::default()
        })
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        *self.block_state.is_write_initial.get(&key).unwrap_or({
            // println!("giving default for is_write_initial, wot? {key:?}");
            &false
        })
    }

    fn load_factory_dep(&mut self, _hash: H256) -> Option<Vec<u8>> {
        None
    }
}
