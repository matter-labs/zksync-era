use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};
use zksync_types::{web3, StorageKey, StorageValue, H256};

use super::ReadStorage;

/// Self-sufficient or almost self-sufficient storage snapshot for a particular VM execution (e.g., executing a single L1 batch).
///
/// `StorageSnapshot` works somewhat similarly to [`InMemoryStorage`](super::InMemoryStorage), but has different semantics
/// and use cases. `InMemoryStorage` is intended to be a modifiable storage to be used primarily in tests / benchmarks.
/// In contrast, `StorageSnapshot` cannot be modified once created and is intended to represent a complete or almost complete snapshot
/// for a particular VM execution. It can serve as a preloaded cache for a certain [`ReadStorage`] implementation
/// that significantly reduces the number of storage accesses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageSnapshot {
    // `Option` encompasses entire map value for more efficient serialization
    storage: HashMap<H256, Option<(H256, u64)>>,
    // `Bytes` are used to have efficient serialization
    factory_deps: HashMap<H256, web3::Bytes>,
}

impl StorageSnapshot {
    /// Creates a new storage snapshot.
    ///
    /// # Arguments
    ///
    /// - `storage` should contain all storage slots accessed during VM execution, i.e. protective reads + initial / repeated writes
    ///   for batch execution, keyed by the hashed storage key. `None` map values correspond to accessed slots without an assigned enum index
    ///   and 0 values. There may be slots w/o an index and non-zero value if the snapshot captures execution from a middle of batch;
    ///   in this case, you should supply `Some(_, 0)`.
    pub fn new(
        storage: HashMap<H256, Option<(H256, u64)>>,
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
    /// Fallback will be called for storage slots / factory deps not in this snapshot (which, if this snapshot
    /// is reasonably constructed, would be a rare occurrence). If `shadow` flag is set, the fallback will be
    /// consulted for *every* operation; this obviously harms performance and is mostly useful for testing.
    ///
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

/// When used as a storage, a snapshot is assumed to be *complete*; [`ReadStorage`] methods will panic when called
/// with storage slots not present in the snapshot.
impl ReadStorage for StorageSnapshot {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let entry = self
            .storage
            .get(&key.hashed_key())
            .unwrap_or_else(|| panic!("attempted to read from unknown storage slot: {key:?}"));
        entry.unwrap_or_default().0
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let entry = self.storage.get(&key.hashed_key()).unwrap_or_else(|| {
            panic!("attempted to check initialness for unknown storage slot: {key:?}")
        });
        entry.is_none_or(|(_, idx)| idx == 0)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.factory_deps.get(&hash).map(|bytes| bytes.0.clone())
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        let entry = self.storage.get(&key.hashed_key()).unwrap_or_else(|| {
            panic!("attempted to get enum index for unknown storage slot: {key:?}")
        });
        entry.and_then(|(_, idx)| (idx > 0).then_some(idx))
    }
}

/// [`StorageSnapshot`] wrapper implementing [`ReadStorage`] trait. Created using [`with_fallback()`](StorageSnapshot::with_fallback()).
///
/// # Why fallback?
///
/// The reason we require a fallback is that it may be difficult to create a 100%-complete snapshot in the general case.
/// E.g., for batch execution, the data is mostly present in Postgres (provided that protective reads are recorded),
/// but in some scenarios, accessed slots may be not recorded anywhere (e.g., if a slot is written to and then reverted in the same block).
/// In practice, there are order of 10 such slots for a mainnet batch with ~5,000 transactions / ~35,000 accessed slots;
/// i.e., snapshots still can provide a good speed-up boost.
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
            .map(|entry| entry.unwrap_or_default().0);
        self.fallback(format_args!("read_value({key:?})"), value, |storage| {
            storage.read_value(key)
        })
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let is_initial = self
            .snapshot
            .storage
            .get(&key.hashed_key())
            .map(Option::is_none);
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
            .map(|entry| entry.map(|(_, idx)| idx));
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
                (H256::repeat_byte(1), Some((H256::from_low_u64_be(1), 10))),
                (
                    H256::repeat_byte(0x23),
                    Some((H256::from_low_u64_be(100), 100)),
                ),
                (H256::repeat_byte(0xff), None),
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
                "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff": null,
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
