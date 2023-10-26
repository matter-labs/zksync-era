use vise::{Counter, Metrics};

use crate::ReadStorage;

use zksync_types::{witness_block_state::WitnessBlockState, StorageKey, StorageValue, H256};

#[derive(Debug, Metrics)]
#[metrics(prefix = "witness_storage")]
struct WitnessStorageMetrics {
    /// Number of unexpected calls when calling `get_enumeration_index` on a witness storage.
    get_enumeration_index_unexpected_call: Counter,
}

#[vise::register]
static METRICS: vise::Global<WitnessStorageMetrics> = vise::Global::new();

/// [`ReadStorage`] implementation backed by binary serialized [`WitnessHashBlockState`].
/// Note that `load_factory_deps` is not used.
/// FactoryDeps data is used straight inside witness generator, loaded with the blob.
#[derive(Debug)]
pub struct WitnessStorage<'a> {
    block_state: WitnessBlockState,
    metrics: &'a WitnessStorageMetrics,
}

impl WitnessStorage<'_> {
    /// Creates a new storage with the provided witness's block state.
    pub fn new(block_state: WitnessBlockState) -> Self {
        Self {
            block_state,
            metrics: &METRICS,
        }
    }
}

impl ReadStorage for WitnessStorage<'_> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        *self
            .block_state
            .read_storage_key
            .get(key)
            .unwrap_or(&H256::default())
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        *self.block_state.is_write_initial.get(key).unwrap_or(&false)
    }

    fn load_factory_dep(&mut self, _hash: H256) -> Option<Vec<u8>> {
        None
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        self.metrics.get_enumeration_index_unexpected_call.inc();
        None
    }
}
