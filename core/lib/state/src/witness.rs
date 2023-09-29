use crate::ReadStorage;

use zksync_types::{
    witness_block_state::WitnessBlockState, L1BatchNumber, StorageKey, StorageValue, H256,
};

/// [`ReadStorage`] implementation backed by binary serialized [`WitnessBlockState`].
/// Note that `load_factory_deps` is not used.
/// FactoryDeps data is used straight inside witness generator, loaded with the blob.
#[derive(Debug)]
pub struct WitnessStorage {
    l1_batch_number: L1BatchNumber,
    witness_block_state: WitnessBlockState,
}

impl WitnessStorage {
    /// Creates a new storage with the provided witness's block state.
    pub fn new(l1_batch_number: L1BatchNumber, witness_block_state: WitnessBlockState) -> Self {
        Self {
            l1_batch_number,
            witness_block_state,
        }
    }

    fn check_cache_value<T: Default>(
        &self,
        cache_value: Option<T>,
        function_call: &str,
        metric_name: &str,
    ) -> T {
        let (val, metric_val) = if let Some(val) = cache_value {
            (val, 0.0)
        } else {
            vlog::error!(
                "{function_call:?} for l1_batch_number={:?} had no precached return value",
                self.l1_batch_number
            );
            (T::default(), 1.0)
        };
        metrics::histogram!(format!("witness_storage.{metric_name:?}"), metric_val);
        val
    }
}

impl ReadStorage for WitnessStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let cache_value = self.witness_block_state.read_storage_key.get(key);
        self.check_cache_value(
            cache_value.copied(),
            &format!("read_value({key:?})"),
            "read_value_cache_miss",
        )
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let cache_value = self.witness_block_state.is_write_initial.get(key);
        self.check_cache_value(
            cache_value.copied(),
            &format!("is_write_initial({key:?})"),
            "is_write_initial_cache_miss",
        )
    }

    fn load_factory_dep(&mut self, _hash: H256) -> Option<Vec<u8>> {
        metrics::histogram!("witness_storage.unexpected_load_factory_dep_call", 1.0);
        None
    }
}
