use crate::ReadStorage;
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};

/// [`ReadStorage`] implementation backed by 2 different backends:
/// source_storage -- backend that will return values for function calls and be the source of truth
/// to_check_storage -- secondary storage, which will verify it's own return values against source_storage
/// Note that if to_check_storage value is different than source value, execution continues and metrics/ logs are emitted.
#[derive(Debug)]
pub struct ShadowStorage<'a, 'b> {
    source_storage: Box<dyn ReadStorage + 'a>,
    to_check_storage: Box<dyn ReadStorage + 'b>,
    l1_batch_number: L1BatchNumber,
}

impl<'a, 'b> ShadowStorage<'a, 'b> {
    /// Creates a new storage using the 2 underlying [`ReadStorage`]s, first as source, the second to be checked against the source.
    pub fn new(
        source_storage: Box<dyn ReadStorage + 'a>,
        to_check_storage: Box<dyn ReadStorage + 'b>,
        l1_batch_number: L1BatchNumber,
    ) -> Self {
        Self {
            source_storage,
            to_check_storage,
            l1_batch_number,
        }
    }
}

impl ReadStorage for ShadowStorage<'_, '_> {
    fn read_value(&mut self, &key: &StorageKey) -> StorageValue {
        let source_value = self.source_storage.read_value(&key);
        let expected_value = self.to_check_storage.read_value(&key);
        let mut metric_value = 0.0;
        if source_value != expected_value {
            metric_value = 1.0;
            vlog::error!("read_value({:?}) -- l1_batch_number={:?} -- expected source={:?} to be equal to to_check={:?}", key, self.l1_batch_number, source_value, expected_value);
        }
        metrics::histogram!("shadow_storage.read_value_mismatch", metric_value);
        source_value
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let source_value = self.source_storage.is_write_initial(key);
        let expected_value = self.to_check_storage.is_write_initial(key);
        let mut metric_value = 0.0;
        if source_value != expected_value {
            metric_value = 1.0;
            vlog::error!("is_write_initial({:?}) -- l1_batch_number={:?} -- expected source={:?} to be equal to to_check={:?}", key, self.l1_batch_number, source_value, expected_value);
        }
        metrics::histogram!("shadow_storage.is_write_initial_mismatch", metric_value);
        source_value
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let source_value = self.source_storage.load_factory_dep(hash);
        let expected_value = self.to_check_storage.load_factory_dep(hash);
        let mut metric_value = 0.0;
        if source_value != expected_value {
            metric_value = 1.0;
            vlog::error!("load_factory_dep({:?}) -- l1_batch_number={:?} -- expected source={:?} to be equal to to_check={:?}", hash, self.l1_batch_number, source_value, expected_value);
        }
        metrics::histogram!("shadow_storage.load_factory_dep_mismatch", metric_value);
        source_value
    }
}
