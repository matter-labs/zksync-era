use std::collections::HashMap;

use zksync_types::{StorageKey, StorageValue, H256};

use super::ReadStorage;

/// Self-sufficient storage snapshot for a particular VM execution (e.g., executing a single L1 batch).
///
/// # Important
///
/// Note that [`ReadStorage`] methods will not panic / log errors etc. if "unexpected" storage slots
/// are accessed during VM execution; instead, it'll return default values for these storage slots. The caller is responsible
/// for ensuring that the snapshot matches VM setup.
#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    storage: HashMap<H256, (H256, u64)>,
    factory_deps: HashMap<H256, Vec<u8>>,
}

impl StorageSnapshot {
    /// Creates a new storage snapshot.
    ///
    /// # Arguments
    ///
    /// - `storage` must contain all storage slots read during VM execution, i.e. protective reads + repeated writes
    ///   for batch execution.
    pub fn new(storage: HashMap<H256, (H256, u64)>, factory_deps: HashMap<H256, Vec<u8>>) -> Self {
        Self {
            storage,
            factory_deps,
        }
    }
}

impl ReadStorage for StorageSnapshot {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.storage
            .get(&key.hashed_key())
            .copied()
            .unwrap_or_default()
            .0
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        !self.storage.contains_key(&key.hashed_key())
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.factory_deps.get(&hash).cloned()
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        Some(self.storage.get(&key.hashed_key())?.1)
    }
}
