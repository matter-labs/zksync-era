use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zksync_types::{web3, StorageKey, StorageValue, H256};

use super::ReadStorage;

/// Self-sufficient storage snapshot for a particular VM execution (e.g., executing a single L1 batch).
///
/// # Important
///
/// Note that [`ReadStorage`] methods will not panic / log errors etc. if "unexpected" storage slots
/// are accessed during VM execution; instead, it'll return default values for these storage slots. The caller is responsible
/// for ensuring that the snapshot matches VM setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSnapshot {
    storage: HashMap<H256, (H256, u64)>,
    // `Bytes` are used to have efficient serialization
    factory_deps: HashMap<H256, web3::Bytes>,
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
            factory_deps: factory_deps
                .into_iter()
                .map(|(hash, bytecode)| (hash, web3::Bytes(bytecode)))
                .collect(),
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
        Some(self.factory_deps.get(&hash)?.0.clone())
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        Some(self.storage.get(&key.hashed_key())?.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializing_snapshot_to_json() {
        let snapshot = StorageSnapshot::new(
            HashMap::from([
                (H256::repeat_byte(1), (H256::from_low_u64_be(1), 10)),
                (H256::repeat_byte(0x23), (H256::from_low_u64_be(100), 100)),
            ]),
            HashMap::from([(H256::repeat_byte(2), (0..32).collect())]),
        );
        let expected_json = serde_json::json!({
            "storage": {
                "0x0101010101010101010101010101010101010101010101010101010101010101": [
                    "0x0000000000000000000000000000000000000000000000000000000000000001",
                    10,
                ],
                "0x2323232323232323232323232323232323232323232323232323232323232323": [
                    "0x0000000000000000000000000000000000000000000000000000000000000064",
                    100,
                ],
            },
            "factory_deps": {
                "0x0202020202020202020202020202020202020202020202020202020202020202":
                    "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
            },
        });
        let actual_json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(actual_json, expected_json);

        let restored: StorageSnapshot = serde_json::from_value(actual_json).unwrap();
        assert_eq!(restored.storage, snapshot.storage);
        assert_eq!(restored.factory_deps, snapshot.factory_deps);
    }
}
