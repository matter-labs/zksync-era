//! General-purpose RocksDB metrics. All metrics code in the crate should be in this module.

use once_cell::sync::Lazy;
use vise::{Buckets, Collector, EncodeLabelSet, Family, Gauge, Histogram, Metrics};

use std::{
    collections::HashMap,
    sync::{Mutex, Weak},
};

use crate::db::RocksDBInner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct DbLabel {
    db: &'static str,
}

impl From<&'static str> for DbLabel {
    fn from(db: &'static str) -> Self {
        Self { db }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct RocksdbLabels {
    db: &'static str,
    cf: &'static str,
}

impl RocksdbLabels {
    pub(crate) fn new(db: &'static str, cf: &'static str) -> Self {
        Self { db, cf }
    }
}

const BYTE_SIZE_BUCKETS: Buckets = Buckets::exponential(65_536.0..=16.0 * 1_024.0 * 1_024.0, 2.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "rocksdb")]
pub(crate) struct RocksdbMetrics {
    /// Size of a serialized `WriteBatch` written to a RocksDB instance.
    #[metrics(buckets = BYTE_SIZE_BUCKETS)]
    write_batch_size: Family<DbLabel, Histogram<usize>>,
}

impl RocksdbMetrics {
    pub(crate) fn report_batch_size(&self, db: &'static str, batch_size: usize) {
        self.write_batch_size[&db.into()].observe(batch_size);
    }
}

#[vise::register]
pub(crate) static METRICS: vise::Global<RocksdbMetrics> = vise::Global::new();

/// Portion of metrics that use a collector.
#[derive(Debug, Metrics)]
#[metrics(prefix = "rocksdb")]
pub(crate) struct RocksdbSizeMetrics {
    /// Estimated size of all live data in the column family of a RocksDB instance.
    pub live_data_size: Family<RocksdbLabels, Gauge<u64>>,
    /// Total size of all SST files in the column family of a RocksDB instance.
    pub total_sst_size: Family<RocksdbLabels, Gauge<u64>>,
    /// Total size of all mem tables in the column family of a RocksDB instance.
    pub total_mem_table_size: Family<RocksdbLabels, Gauge<u64>>,
    /// Total size of block cache in the column family of a RocksDB instance.
    pub block_cache_size: Family<RocksdbLabels, Gauge<u64>>,
    /// Total size of index and Bloom filters in the column family of a RocksDB instance.
    pub index_and_filters_size: Family<RocksdbLabels, Gauge<u64>>,
}

/// Weak refs to DB instances registered using [`RocksdbSizeMetrics::register()`].
static INSTANCES: Lazy<Mutex<HashMap<&'static str, Weak<RocksDBInner>>>> =
    Lazy::new(Mutex::default);

impl RocksdbSizeMetrics {
    pub(crate) fn register(db_name: &'static str, instance: Weak<RocksDBInner>) {
        #[vise::register]
        static COLLECTOR: Collector<RocksdbSizeMetrics> = Collector::new();

        INSTANCES
            .lock()
            .expect("instances are poisoned")
            .insert(db_name, instance);
        // Set up the collector. This will return an error on subsequent calls, but we're OK with it.
        COLLECTOR.before_scrape(Self::scrape).ok();
    }

    fn scrape() -> Self {
        let metrics = Self::default();
        // Remove instances that have been dropped, and collect metrics for the alive instances.
        INSTANCES
            .lock()
            .expect("instances are poisoned")
            .retain(|_, instance| {
                if let Some(instance) = instance.upgrade() {
                    instance.report_sizes(&metrics);
                    true
                } else {
                    false
                }
            });
        metrics
    }
}
