use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};
use zksync_types::{web3, StorageKey, StorageValue, H256};

use super::ReadStorage;

/// Self-sufficient storage snapshot for a particular VM execution (e.g., executing a single L1 batch).
///
/// `StorageSnapshot` works similarly to [`InMemoryStorage`](super::InMemoryStorage), but has different semantics
/// and use cases. `InMemoryStorage` is intended to be a modifiable storage to be used primarily in tests / benchmarks.
/// In contrast, `StorageSnapshot` cannot be modified once created and is intended to represent a snapshot
/// for a particular VM execution.
///
/// # Important
///
/// Note that [`ReadStorage`] methods will not panic / log errors etc. if "unexpected" storage slots
/// are accessed during VM execution; instead, it'll return default values for these storage slots. The caller is responsible
/// for ensuring that the snapshot matches VM setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSnapshot {
    storage: HashMap<H256, (H256, Option<u64>)>,
    // `Bytes` are used to have efficient serialization
    factory_deps: HashMap<H256, web3::Bytes>,
}

impl StorageSnapshot {
    /// Creates a new storage snapshot.
    ///
    /// # Arguments
    ///
    /// - `storage` should contain all storage slots accessed during VM execution, i.e. protective reads + initial / repeated writes
    ///   for batch execution.
    pub fn new(
        storage: HashMap<H256, (H256, Option<u64>)>,
        factory_deps: HashMap<H256, Vec<u8>>,
    ) -> Self {
        Self {
            storage,
            factory_deps: factory_deps
                .into_iter()
                .map(|(hash, bytecode)| (hash, web3::Bytes(bytecode)))
                .collect(),
        }
    }

    /// Creates a [`ReadStorage`] implementation based on this snapshot and the provided fallback implementation.
    /// The caller is responsible for ensuring that the fallback actually corresponds to the snapshot.
    pub fn with_fallback<S: ReadStorage>(
        self,
        fallback: S,
        shadow: bool,
    ) -> StorageWithSnapshot<S> {
        StorageWithSnapshot {
            snapshot: self,
            fallback,
            shadow,
        }
    }
}

/// [`StorageSnapshot`] wrapper implementing [`ReadStorage`] trait. Created using [`with_fallback()`](StorageSnapshot::with_fallback()).
#[derive(Debug)]
pub struct StorageWithSnapshot<S> {
    snapshot: StorageSnapshot,
    fallback: S,
    shadow: bool,
}

impl<S: ReadStorage> StorageWithSnapshot<S> {
    fn fallback<T: fmt::Debug + PartialEq>(
        &mut self,
        operation: fmt::Arguments<'_>,
        value: Option<T>,
        f: impl FnOnce(&mut S) -> T,
    ) -> T {
        if let Some(value) = value {
            if self.shadow {
                let fallback_value = f(&mut self.fallback);
                assert_eq!(value, fallback_value, "mismatch in {operation} output");
            }
            return value;
        }
        tracing::trace!("Output for {operation} is missing in snapshot");
        f(&mut self.fallback)
    }
}

impl<S: ReadStorage> ReadStorage for StorageWithSnapshot<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let value = self
            .snapshot
            .storage
            .get(&key.hashed_key())
            .map(|(value, _)| *value);
        self.fallback(format_args!("read_value({key:?})"), value, |storage| {
            storage.read_value(key)
        })
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let is_initial = self
            .snapshot
            .storage
            .get(&key.hashed_key())
            .map(|(_, enum_index)| enum_index.is_none());
        self.fallback(
            format_args!("is_write_initial({key:?})"),
            is_initial,
            |storage| storage.is_write_initial(key),
        )
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let dep = self
            .snapshot
            .factory_deps
            .get(&hash)
            .map(|dep| Some(dep.0.clone()));
        self.fallback(format_args!("load_factory_dep({hash})"), dep, |storage| {
            storage.load_factory_dep(hash)
        })
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        let enum_index = self
            .snapshot
            .storage
            .get(&key.hashed_key())
            .map(|(_, idx)| *idx);
        self.fallback(
            format_args!("get_enumeration_index({key:?})"),
            enum_index,
            |storage| storage.get_enumeration_index(key),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializing_snapshot_to_json() {
        let snapshot = StorageSnapshot::new(
            HashMap::from([
                (H256::repeat_byte(1), (H256::from_low_u64_be(1), Some(10))),
                (
                    H256::repeat_byte(0x23),
                    (H256::from_low_u64_be(100), Some(100)),
                ),
                (H256::repeat_byte(0xff), (H256::zero(), None)),
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
                "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff": [
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                    null
                ]
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
