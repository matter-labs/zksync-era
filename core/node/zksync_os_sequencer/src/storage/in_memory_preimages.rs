use std::sync::Arc;
use dashmap::DashMap;
use zk_ee::utils::Bytes32;
use zk_os_forward_system::run::PreimageSource;

/// An append-only, thread-safe store of preimages (Bytes32 → Vec<u8>).
///
/// Internally uses a DashMap for concurrent readers/writers.
/// On read, returns a fresh Vec<u8> clone of the stored value.
#[derive(Clone, Debug)]
pub struct InMemoryPreimages {
    pub map: Arc<DashMap<Bytes32, Vec<u8>>>,
}

impl InMemoryPreimages {
    /// Create an empty PreimageSource.
    pub fn empty() -> Self {
        InMemoryPreimages {
            map: Arc::new(DashMap::new()),
        }
    }

    /// Insert a single preimage if absent.
    /// If the key already exists, this does nothing.
    pub fn add(&self, key: Bytes32, preimage: Vec<u8>) {
        self.map.entry(key).or_insert(preimage);

    }

    /// Insert multiple preimages at once.
    ///
    /// Each `(key, preimage)` is added if the key is not already present.
    /// This batch insertion is safe for concurrent use.
    pub fn add_many<I>(&self, entries: I)
    where
        I: IntoIterator<Item = (Bytes32, Vec<u8>)>,
    {
        for (key, preimage) in entries {
            // tracing::info!("Adding preimage for key: {:?}", key);
            // or_insert avoids overwriting existing entries
            self.map.entry(key).or_insert(preimage);
        }
    }

    /// Returns true if the store contains the given key.
    pub fn contains(&self, key: &Bytes32) -> bool {
        self.map.contains_key(key)
    }

    /// Number of preimages currently stored.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
