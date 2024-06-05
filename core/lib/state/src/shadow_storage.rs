use vise::{Counter, Metrics};
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};

use crate::ReadStorage;

#[allow(clippy::struct_field_names)]
#[derive(Debug, Metrics)]
#[metrics(prefix = "shadow_storage")]
struct ShadowStorageMetrics {
    /// Number of mismatches when reading a value from a shadow storage.
    read_value_mismatch: Counter,
    /// Number of mismatches when calling `is_write_initial()` on a shadow storage.
    is_write_initial_mismatch: Counter,
    /// Number of mismatches when calling `load_factory_dep()` on a shadow storage.
    load_factory_dep_mismatch: Counter,
    /// Number of mismatches when calling `get_enumeration_index` on a shadow storage.
    get_enumeration_index_mismatch: Counter,
}

#[vise::register]
static METRICS: vise::Global<ShadowStorageMetrics> = vise::Global::new();

/// [`ReadStorage`] implementation backed by 2 different backends:
/// source_storage -- backend that will return values for function calls and be the source of truth
/// to_check_storage -- secondary storage, which will verify it's own return values against source_storage
/// Note that if to_check_storage value is different than source value, execution continues and metrics/ logs are emitted.
#[derive(Debug)]
pub struct ShadowStorage<'a> {
    source_storage: Box<dyn ReadStorage + 'a>,
    to_check_storage: Box<dyn ReadStorage + 'a>,
    metrics: &'a ShadowStorageMetrics,
    l1_batch_number: L1BatchNumber,
}

impl<'a> ShadowStorage<'a> {
    /// Creates a new storage using the 2 underlying [`ReadStorage`]s, first as source, the second to be checked
    /// against the source.
    pub fn new(
        source_storage: Box<dyn ReadStorage + 'a>,
        to_check_storage: Box<dyn ReadStorage + 'a>,
        l1_batch_number: L1BatchNumber,
    ) -> Self {
        Self {
            source_storage,
            to_check_storage,
            metrics: &METRICS,
            l1_batch_number,
        }
    }
}

impl ReadStorage for ShadowStorage<'_> {
    fn read_value(&mut self, &key: &StorageKey) -> StorageValue {
        let source_value = self.source_storage.read_value(&key);
        let expected_value = self.to_check_storage.read_value(&key);
        if source_value != expected_value {
            self.metrics.read_value_mismatch.inc();
            tracing::error!(
                "read_value({key:?}) -- l1_batch_number={:?} -- expected source={source_value:?} \
                 to be equal to to_check={expected_value:?}",
                self.l1_batch_number
            );
        }
        source_value
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let source_value = self.source_storage.is_write_initial(key);
        let expected_value = self.to_check_storage.is_write_initial(key);
        if source_value != expected_value {
            self.metrics.is_write_initial_mismatch.inc();
            tracing::error!(
                "is_write_initial({key:?}) -- l1_batch_number={:?} -- expected source={source_value:?} \
                 to be equal to to_check={expected_value:?}",
                self.l1_batch_number
            );
        }
        source_value
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let source_value = self.source_storage.load_factory_dep(hash);
        let expected_value = self.to_check_storage.load_factory_dep(hash);
        if source_value != expected_value {
            self.metrics.load_factory_dep_mismatch.inc();
            tracing::error!(
                "load_factory_dep({hash:?}) -- l1_batch_number={:?} -- expected source={source_value:?} \
                 to be equal to to_check={expected_value:?}",
                 self.l1_batch_number
            );
        }
        source_value
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        let source_value = self.source_storage.get_enumeration_index(key);
        let expected_value = self.to_check_storage.get_enumeration_index(key);
        if source_value != expected_value {
            tracing::error!(
                "get_enumeration_index({:?}) -- l1_batch_number={:?} -- expected source={:?} to be equal to \
                to_check={:?}", key, self.l1_batch_number, source_value, expected_value
            );

            self.metrics.get_enumeration_index_mismatch.inc();
        }
        source_value
    }
}

// TODO: Add unit tests when we swap metrics crate; blocked by: https://linear.app/matterlabs/issue/QIT-3/rework-metrics-approach
