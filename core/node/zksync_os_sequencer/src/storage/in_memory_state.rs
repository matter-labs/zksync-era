use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use zk_ee::utils::Bytes32;
use dashmap::DashMap;
use zk_os_basic_system::system_implementation::flat_storage_model::AccountProperties;
use zk_os_forward_system::run::{ReadStorage, ReadStorageTree, StorageWrite};
use zksync_web3_decl::types::Bytes;
use crate::{BLOCKS_TO_RETAIN};

/// In-memory storage tracking a mutable base state,
/// a bounded history of diffs up to `max_diffs`,
/// and the highest block number seen.
#[derive(Clone, Debug)]
pub struct InMemoryStorage {
    /// The height at which base_state is valid. Updated when diffs are compacted.
    pub base_block: Arc<AtomicU64>,
    /// Mutable base state at `base_block`.
    pub base_state: Arc<DashMap<Bytes32, Bytes32>>,
    /// Thread-safe diffs: block_number -> state changes for that block.
    pub diffs: Arc<DashMap<u64, Arc<HashMap<Bytes32, Bytes32>>>>,

    /// Maximum number of diffs to retain.
    max_diffs: usize,
}

impl InMemoryStorage {
    /// Creates a new InMemoryStorage with the given base block, initial state, and diff capacity.
    pub fn empty() -> Self {
        Self {
            base_block: Arc::new(AtomicU64::new(0)),
            base_state: Arc::new(DashMap::new()),
            diffs: Arc::new(DashMap::new()),
            max_diffs: BLOCKS_TO_RETAIN,
        }
    }

    /// Adds a diff for a specific block number. Safe for concurrent use.
    /// If the number of stored diffs exceeds `max_diffs`, the oldest diff is
    /// compacted into the base state and removed.
    /// Also updates the highest_block to `block`.
    pub fn add_diff(&self, block: u64, storage_writes: Vec<StorageWrite>) {
        let diff = storage_writes.into_iter()
            .map(|write| (write.key, write.value))
            .collect::<HashMap<Bytes32, Bytes32>>();
        // Insert new diff
        self.diffs.insert(block, Arc::new(diff));
        // If too many diffs, compact oldest
        while self.diffs.len() > self.max_diffs {
            // Find smallest block number
            if let Some((oldest_block, _)) = self.diffs.iter()
                .map(|e| (*e.key(), e.value().clone()))
                .min_by_key(|e| e.0)
            {
                // Remove and compact into base_state
                if let Some((_, old_diff)) = self.diffs.remove(&oldest_block) {
                    // Merge each key/value into base_state
                    for (k, v) in Arc::try_unwrap(old_diff).unwrap_or_else(|arc| (*arc).clone()) {
                        self.base_state.insert(k, v);
                    }
                    // Advance base_block
                    self.base_block.store(oldest_block, Ordering::SeqCst);
                }
            } else {
                break;
            }
        }
    }
}