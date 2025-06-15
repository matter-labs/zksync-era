//! storage.rs  –  hot diff-ring in RAM + cold base in RocksDB
//! Requires the RocksDB wrapper you already have in scope.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;
use dashmap::DashMap;
use zk_ee::utils::Bytes32;
use zk_os_forward_system::run::{ReadStorage, ReadStorageTree, StorageWrite};
use zksync_storage::db::NamedColumnFamily;
use zksync_storage::RocksDB;
use crate::BLOCKS_TO_RETAIN;
use crate::execution::metrics::{STORAGE_VIEW_METRICS};
use crate::storage::persistent_storage_map::{PersistentStorageMap, StorageMapCF};
use crate::storage::StorageView;

/* ------------------------------------------------------------------ */
/*  Per-block diff with tiny Bloom                                    */
/* ------------------------------------------------------------------ */
#[derive(Debug)]
pub struct Diff {
    pub bloom: [u8; 32],                      // 256 bits
    pub map: HashMap<Bytes32, Bytes32>,
}
impl Diff {
    fn new(writes: Vec<StorageWrite>) -> Self {
        let mut bloom = [0u8; 32];
        let mut map = HashMap::with_capacity(writes.len());
        for w in writes {
            Self::bloom_set(&mut bloom, &w.key);
            map.insert(w.key, w.value);
        }
        Self { bloom, map }
    }

    #[inline]
    fn bloom_set(b: &mut [u8; 32], key: &Bytes32) {
        let bytes = key.as_u8_array();
        for idx in [bytes[0], bytes[1]] {         // 2 hash functions
            let bit = idx & 7;
            let byte = (idx >> 3) as usize;
            b[byte] |= 1 << bit;
        }
    }

    #[inline]
    pub fn might_contain(&self, key: &Bytes32) -> bool {
        let bytes = key.as_u8_array();
        for idx in [bytes[0], bytes[1]] {
            let bit = idx & 7;
            let byte = (idx >> 3) as usize;
            if self.bloom[byte] & (1 << bit) == 0 {
                return false;
            }
        }
        true
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

    /// block in rocksDB is no older than
    pub persistent_block_lower_bound: Arc<AtomicU64>,
    /// block in rocksDB is no newer than
    pub persistent_block_upper_bound: Arc<AtomicU64>,
    /// RocksDB handle for the persistent base - cheap to clone
    pub persistent_storage_map: PersistentStorageMap,
}

impl StorageMap {
    pub fn view_at(
        &self,
        block_number: u64,
    ) -> anyhow::Result<StorageMapView> {
        let persistent_block_upper_bound = self.persistent_block_upper_bound.load(Ordering::SeqCst);
        let persistent_block_lower_bound = self.persistent_block_lower_bound.load(Ordering::SeqCst);
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
        tracing::info!("Creating StorageMapView for block {} with pesistance bounds {} to {}", block_number, persistent_block_lower_bound, persistent_block_upper_bound);

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
            persistent_block_lower_bound: Arc::new(rocksdb_block.into()),
            persistent_block_upper_bound: Arc::new(rocksdb_block.into()),
            persistent_storage_map,
        }
    }

    /// Adds a diff for block `block`.  When the ring exceeds `max_diffs`,
    /// compacts the oldest diff into RocksDB synchronously.
    pub fn add_diff(&self, block: u64, writes: Vec<StorageWrite>) {
        let started_at = Instant::now();
        let persistent_block_upper_bound = self.persistent_block_upper_bound.load(Ordering::SeqCst);
        assert!(
            block > persistent_block_upper_bound,
            "StorageMap: cannot add diff for block {} - it's below what's already persisted: {}",
            block,
            persistent_block_upper_bound
        );

        let diff = Diff::new(writes);           // use builder with bloom
        self.diffs.insert(block, Arc::new(diff));

        // self.block_number_upper_bound.store(block, Ordering::SeqCst);

        STORAGE_VIEW_METRICS.storage_add_diff[&"total"].observe(started_at.elapsed());

        // todo: doing sync for debug - will need to be async
        self.compact();
    }

    // todo: will change when we use ordered map
    fn max_block_in_diffs(&self) -> u64 {
        self.diffs.iter()
            .max_by_key(|r| *r.key())
            .map(|r| *r.key())
            .unwrap_or(0u64)
    }

    fn compact(&self) {
        let started_at = Instant::now();

        let can_compact_until = self.max_block_in_diffs().saturating_sub(BLOCKS_TO_RETAIN as u64);
        let (prev_persisted, initial_upper) = (
            self.persistent_block_lower_bound.load(Ordering::SeqCst),
            self.persistent_block_upper_bound.load(Ordering::SeqCst),
        );
        assert_eq!(
            prev_persisted,
            initial_upper,
            "StorageMap: persistent bounds must be equal when starting compaction, got: {} and {}",
            prev_persisted,
            initial_upper,
        );
        if can_compact_until <= initial_upper {
            return;
        }

        tracing::info!(
            "Compacting at least {} diffs: can_compact_until: {}, current: {}",
            can_compact_until - prev_persisted,
            can_compact_until,
            prev_persisted
        );

        // note: this assumes there are no active StorageView below can_compact_until
        // no NEW storage views will be created for blocks below block_number_upper_bound
        // for later - it can be that rocksDB is at block N, while diffs still contain lower blocks
        // this is OK - since keys from those diffs are also reflected in rocksDB
        self.persistent_block_upper_bound.store(can_compact_until, Ordering::SeqCst);

        let mut map: HashMap<Bytes32, Bytes32> = Default::default();
        for bn in (prev_persisted + 1..=can_compact_until) {
            if let Some(diff) = self.diffs.get(&bn) {
                diff.value().map.iter().for_each(|(k, v)| {
                    map.insert(*k, *v);
                });
            } else {
                tracing::warn!(
                    "StorageMap: compacting diffs, but no diff found for block {}",
                    bn
                );
            }
        }

        tracing::info!(
            "Compacting {} unique keys",
            map.len(),
        );

        self.persistent_storage_map.compact_sync(can_compact_until, map);
        self.persistent_block_lower_bound.store(can_compact_until, Ordering::SeqCst);

        for block_number in (prev_persisted + 1..=can_compact_until).rev() {
            if let Some(diff) = self.diffs.remove(&block_number) {
                tracing::debug!("Compacted diff for block {}", block_number);
            } else {
                tracing::warn!("No diff found for block {}", block_number);
            }
        }

        STORAGE_VIEW_METRICS.storage_persistence[&"compact"].observe(started_at.elapsed());
    }
}


impl ReadStorage for StorageMapView {
    /// Reads `key` by scanning block diffs from `block - 1` down to `base_block + 1`,
    /// then falling back to the persistence
    fn read(&mut self, key: Bytes32) -> Option<Bytes32> {
        let started_at = Instant::now();
        for bn in (self.base_block + 1..self.block).rev() {
            if let Some(diff) = self.diffs.get(&bn) {
                let res = diff.map.get(&key);
                let bloom = diff.might_contain(&key);

                let outcome = match (res, bloom) {
                    (Some(_), true) => "hit",
                    (None, true) => "false_pos",
                    (None, false) => "true_pos",
                    (Some(_), false) => {
                        panic!("bloom failure: res: {:?}, bloom: {}", res, bloom);
                    }
                };

                STORAGE_VIEW_METRICS.storage_bloom_outcome[&outcome].inc();

                if let Some(value) = res {
                    STORAGE_VIEW_METRICS.storage_access_latency[&"diff"].observe(started_at.elapsed());
                    STORAGE_VIEW_METRICS.storage_access_diffs_scanned[&"diff"].observe(self.block - bn);
                    return Some(*value);
                }
            } else {
                tracing::warn!("StorageMapView for {} (base block {}) read key: no diff found for block {}",
                    self.block,
                    self.base_block,
                    bn
                );
            }
        }

        // Fallback to base_state
        let r = self.persistent_storage_map.get_from_rocks(key);

        STORAGE_VIEW_METRICS.storage_access_latency[&"base"].observe(started_at.elapsed());
        STORAGE_VIEW_METRICS.storage_access_diffs_scanned[&"base"].observe(self.block - self.base_block);
        r
    }
}