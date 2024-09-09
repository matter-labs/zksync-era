use zksync_types::{StorageKey, StorageValue, H256};
use zksync_vm_interface::storage::StorageWithSnapshot;

use super::metrics::{AccessKind, SNAPSHOT_METRICS};
use crate::{interface::ReadStorage, PostgresStorage};

/// Wrapper around [`PostgresStorage`] used to track frequency of fallback access.
#[derive(Debug)]
pub struct FallbackStorage<'a>(PostgresStorage<'a>);

impl<'a> From<PostgresStorage<'a>> for FallbackStorage<'a> {
    fn from(storage: PostgresStorage<'a>) -> Self {
        Self(storage)
    }
}

impl ReadStorage for FallbackStorage<'_> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let latency = SNAPSHOT_METRICS.fallback_access_latency[&AccessKind::ReadValue].start();
        let output = self.0.read_value(key);
        latency.observe();
        output
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let latency = SNAPSHOT_METRICS.fallback_access_latency[&AccessKind::IsWriteInitial].start();
        let output = self.0.is_write_initial(key);
        latency.observe();
        output
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let latency = SNAPSHOT_METRICS.fallback_access_latency[&AccessKind::LoadFactoryDep].start();
        let output = self.0.load_factory_dep(hash);
        latency.observe();
        output
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        let latency =
            SNAPSHOT_METRICS.fallback_access_latency[&AccessKind::GetEnumerationIndex].start();
        let output = self.0.get_enumeration_index(key);
        latency.observe();
        output
    }
}

/// Snapshot-backed storage used for batch processing.
pub type SnapshotStorage<'a> = StorageWithSnapshot<FallbackStorage<'a>>;
