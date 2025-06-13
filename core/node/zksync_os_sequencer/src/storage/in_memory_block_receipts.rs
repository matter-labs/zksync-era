// in_memory_block_receipts.rs

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use dashmap::DashMap;
use zk_os_forward_system::run::BatchOutput;
use crate::BLOCKS_TO_RETAIN;

/// In-memory store of the most recent N `BatchOutput`s, keyed by block number.
///
/// Assumes that inserts happen strictly in increasing block order (always
/// for `latest_block + 1`), so eviction can be done by block arithmetic.
#[derive(Clone, Debug)]
pub struct InMemoryBlockReceipts {
    /// Maximum number of entries to retain.
    capacity: usize,
    /// Map from block number → `BatchOutput`.
    receipts: Arc<DashMap<u64, BatchOutput>>,
}

impl InMemoryBlockReceipts {
    /// Create a new in-memory receipts store retaining up to `capacity` items.
    pub fn empty() -> Self {
        InMemoryBlockReceipts {
            capacity: BLOCKS_TO_RETAIN,
            receipts: Arc::new(DashMap::new()),
        }
    }

    /// Insert the `BatchOutput` for `block`.
    ///
    /// Must be called with `block == latest_block() + 1`. Evicts the
    /// oldest entry if we exceed `capacity`.
    pub fn insert(&self, block: u64, output: BatchOutput) {
        // Store the new receipt
        self.receipts.insert(block, output);
        // Evict the oldest if over capacity
        if block > self.capacity as u64 {
            let to_evict = block - self.capacity as u64;
            self.receipts.remove(&to_evict);
        }
    }

    /// Retrieve the `BatchOutput` for `block`, if still retained.
    pub fn get(&self, block: u64) -> Option<BatchOutput> {
        self.receipts.get(&block).map(|r| r.value().clone())
    }

    /// Number of receipts currently stored.
    pub fn len(&self) -> usize {
        self.receipts.len()
    }
}
