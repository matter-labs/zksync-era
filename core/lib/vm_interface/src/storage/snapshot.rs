use std::collections::HashMap;

use zksync_types::{StorageKey, StorageValue, H256};

use super::ReadStorage;

#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    storage: HashMap<H256, (H256, u64)>,
    factory_deps: HashMap<H256, Vec<u8>>,
}

impl StorageSnapshot {
    // FIXME: document what `storage` is
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
