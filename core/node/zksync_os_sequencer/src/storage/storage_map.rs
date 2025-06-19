//! storage.rs  –  hot diff-ring in RAM + cold base in RocksDB
//! Requires the RocksDB wrapper you already have in scope.

use crate::execution::metrics::STORAGE_VIEW_METRICS;
use crate::storage::persistent_storage_map::{PersistentStorageMap, StorageMapCF};
use crate::storage::StorageView;
use crate::BLOCKS_TO_RETAIN;
use dashmap::DashMap;
use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::time::Instant;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use zk_ee::utils::Bytes32;
use zk_os_forward_system::run::{ReadStorage, ReadStorageTree, StorageWrite};
use zksync_storage::db::NamedColumnFamily;
use zksync_storage::RocksDB;
use zksync_web3_decl::jsonrpsee::server::middleware::http::Port::Default;

#[derive(Debug)]
pub struct Diff {
    pub map: HashMap<Bytes32, Bytes32>,
}
impl Diff {
    fn new(writes: Vec<StorageWrite>) -> Self {
        Self {
            map: writes.into_iter().map(|w| (w.key, w.value)).collect(),
        }
    }
}
//
/// Storage View valid for a specific block
#[derive(Debug, Clone)]
pub struct StorageMapView {
    /// Block number for which this view is valid.
    block: u64,
    /// Block preceding the first block in diffs
    /// note: it's possible that persistence will be compacted for blocks after `base_block`
    /// and diffs removed from memory - but that's OK - as long as diffs around `block` are not compacted
    // todo: should infer from diffs - other data structure?
    base_block: u64,
    /// All diffs after base_block.
    // todo: older diffs may be dropped from here - consider counting active StorageMapView's?
    diffs: Arc<DashMap<u64, Arc<Diff>>>,
    /// persistence for cases when value is not in diffs
    persistent_storage_map: PersistentStorageMap,
}

/// note: term `block_number` is used for the block at which FINISH the mapping is valid
/// (that is, it can provide execution environment only for block `block_number + 1`)
#[derive(Debug, Clone)]
pub struct StorageMap {
    /// block → diff map (last K blocks).
    pub diffs: Arc<DashMap<u64, Arc<Diff>>>,

    /// RocksDB handle for the persistent base - cheap to clone
    pub persistent_storage_map: PersistentStorageMap,
}

impl StorageMap {
    // todo: reconsider concurrency scheme
    pub fn view_at(&self, block_number: u64) -> anyhow::Result<StorageMapView> {
        let persistent_block_upper_bound = self
            .persistent_storage_map
            .persistent_block_upper_bound
            .load(Ordering::Relaxed);
        let persistent_block_lower_bound = self
            .persistent_storage_map
            .persistent_block_lower_bound
            .load(Ordering::Relaxed);
        // we cannot provide keys for block N when it's already compacted
        // because view_at(N) should return view for the BEGINNING of block N
        if (block_number <= persistent_block_upper_bound) {
            return Err(anyhow::anyhow!(
                "Cannot create StorageView for potentially compacted block {} (potentially compacted until {}, at least until {})",
                block_number,
                persistent_block_upper_bound,
                persistent_block_lower_bound
            ));
        }
        // tracing::info!("Creating StorageMapView for block {} with pesistance bounds {} to {}", block_number, persistent_block_lower_bound, persistent_block_upper_bound);

        Ok(StorageMapView {
            block: block_number,
            // it's important to use lower_bound here since later blocks are not guaranteed to be in rocksDB yet
            base_block: persistent_block_lower_bound,
            diffs: self.diffs.clone(),
            persistent_storage_map: self.persistent_storage_map.clone(),
        })
    }

    pub fn new(persistent_storage_map: PersistentStorageMap) -> Self {
        let rocksdb_block = persistent_storage_map.rocksdb_block_number();

        tracing::info!("Initializing with RocksDB at: {}", rocksdb_block);

        Self {
            diffs: Arc::new(DashMap::new()),
            persistent_storage_map,
        }
    }

    /// Adds a diff for block `block`.  When the ring exceeds `max_diffs`,
    /// compacts the oldest diff into RocksDB synchronously.
    pub fn add_diff(&self, block: u64, writes: Vec<StorageWrite>) {
        let started_at = Instant::now();
        let persistent_block_upper_bound =
            self.persistent_storage_map.persistent_block_upper_bound();
        assert!(
            block > persistent_block_upper_bound,
            "StorageMap: cannot add diff for block {} - it's below what's already persisted: {}",
            block,
            persistent_block_upper_bound
        );

        let diff = Diff::new(writes); 
        self.diffs.insert(block, Arc::new(diff));

        STORAGE_VIEW_METRICS.storage_add_diff.observe(started_at.elapsed());
    }

    // todo: will change when we use ordered map
    fn max_block_in_diffs(&self) -> u64 {
        self.diffs
            .iter()
            .max_by_key(|r| *r.key())
            .map(|r| *r.key())
            .unwrap_or(0u64)
    }

    pub fn compact(&self) {
        let can_compact_until = self
            .max_block_in_diffs()
            .saturating_sub(BLOCKS_TO_RETAIN as u64);
        let current_latest_persisted_block =
            self.persistent_storage_map.persistent_block_upper_bound();

        if can_compact_until <= current_latest_persisted_block {
            tracing::debug!(
                "can_compact_until: {}, last_persisted: {}",
                can_compact_until,
                current_latest_persisted_block,
            );
            return;
        }

        let compacted_diffs_to_compact = self
            .collect_diffs_range(current_latest_persisted_block + 1, can_compact_until)
            .expect("cannot compact block range: one of the diffs is missing");

        tracing::info!(
            "can_compact_until: {}, last_persisted: {} - Compacting {} diffs, {} unique keys",
            can_compact_until,
            current_latest_persisted_block,
            can_compact_until - current_latest_persisted_block,
            compacted_diffs_to_compact.len()
        );

        self.persistent_storage_map
            .compact_sync(can_compact_until, compacted_diffs_to_compact);

        for block_number in (current_latest_persisted_block + 1..=can_compact_until).rev() {
            // todo: will this deadlock if we have StorageView holding a reference to this diff?
            if let Some(diff) = self.diffs.remove(&block_number) {
                tracing::debug!("Compacted diff for block {}", block_number);
            } else {
                panic!("No diff found for block {} while compacting", block_number);
            }
        }
    }

    /// Aggregates all key-value updates between `from` and `to` (inclusive).
    pub fn collect_diffs_range(
        &self,
        from: u64,
        to: u64,
    ) -> anyhow::Result<HashMap<Bytes32, Bytes32>> {
        let mut aggregated_map = HashMap::new();

        for block_number in from..=to {
            match self.diffs.get(&block_number) {
                Some(diff) => aggregated_map.extend(diff.value().map.iter()),
                None => {
                    return Err(anyhow::anyhow!(
                        "StorageMap: compacting diffs, but no diff found for block {}",
                        block_number
                    ))
                }
            }
        }

        Ok(aggregated_map)
    }
}

impl ReadStorage for StorageMapView {
    /// Reads `key` by scanning block diffs from `block - 1` down to `base_block + 1`,
    /// then falling back to the persistence
    fn read(&mut self, key: Bytes32) -> Option<Bytes32> {
        let latency_diffs = STORAGE_VIEW_METRICS.storage_access_latency[&"diff"].start();
        let latency_total = STORAGE_VIEW_METRICS.storage_access_latency[&"total"].start();

        for bn in (self.base_block + 1..self.block).rev() {
            if let Some(diff) = self.diffs.get(&bn) {
                let res = diff.map.get(&key);
                if let Some(value) = res {
                    latency_diffs.observe();
                    latency_total.observe();
                    STORAGE_VIEW_METRICS.storage_access_diffs_scanned
                        .observe(self.block - bn);
                    return Some(*value);
                }
            } else {
                tracing::debug!(
                    "StorageMapView for {} (base block {}) read key: no diff found for block {}",
                    self.block,
                    self.base_block,
                    bn
                );
                // this means this diff is compacted - and so are the diffs before - no point in continuing iteration
                // this is fine as long as the compaction target is below `self.block`.
                // This is currently not checked - but assumed to be true as long as storage views are short-lived
                // todo: add this check
                break;
            }
        }

        latency_diffs.observe();
        STORAGE_VIEW_METRICS.storage_access_diffs_scanned
            .observe(self.block - self.base_block);

        let latency_base = STORAGE_VIEW_METRICS.storage_access_latency[&"base"].start();
        // Fallback to base_state
        let r = self.persistent_storage_map.get_from_rocks(key);
        latency_base.observe();
        latency_total.observe();
        r
    }
}
