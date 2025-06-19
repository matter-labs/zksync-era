use std::time::Duration;
use vise::{Histogram, LabeledFamily, EncodeLabelSet, Family, Gauge, Metrics, Unit, Buckets, Counter};
use zksync_types::prover_dal::WitnessJobStatusFailed;

const LATENCIES_FAST: Buckets = Buckets::exponential(0.0000001..=1.0, 2.0);
const BLOCKS_SCANNED: Buckets = Buckets::linear(1.0..=1000.0, 100.0);
const STORAGE_WRITES: Buckets = Buckets::exponential(1.0..=1000.0, 1.7);


#[derive(Debug, Metrics)]
pub struct ExecutionMetrics {
    // todo: maybe split off performance metrics into a separate struct?
    /// TPS
    #[metrics(labels = ["command"])]
    pub executed_transactions: LabeledFamily<&'static str, Counter>,

    #[metrics(labels = ["stage"])]
    pub sealed_block: LabeledFamily<&'static str, Gauge<u64>>,

    #[metrics(unit = Unit::Seconds, labels = ["stage"], buckets = LATENCIES_FAST)]
    pub block_execution_stages: LabeledFamily<&'static str, Histogram<Duration>>,

    #[metrics(buckets = STORAGE_WRITES)]
    pub storage_writes_per_block: Histogram<u64>,
}

#[derive(Debug, Metrics)]
pub struct StorageViewMetrics {
    #[metrics(unit = Unit::Seconds, labels = ["stage"], buckets = LATENCIES_FAST)]
    pub storage_access_latency: LabeledFamily<&'static str, Histogram<Duration>>,

    #[metrics(buckets = BLOCKS_SCANNED)]
    pub storage_access_diffs_scanned: Histogram<u64>,

    #[metrics(labels = ["outcome"])]
    pub storage_bloom_outcome: LabeledFamily<&'static str, Counter<u64>>,

    #[metrics(unit = Unit::Seconds, buckets = LATENCIES_FAST)]
    pub storage_add_diff: Histogram<Duration>,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "storage_map_rocks_db")]
pub struct StorageMapRocksDBMetrics {
    #[metrics(unit = Unit::Seconds, labels = ["stage"], buckets = LATENCIES_FAST)]
    pub get: LabeledFamily<&'static str, Histogram<Duration>>,

    #[metrics(unit = Unit::Seconds, buckets = LATENCIES_FAST)]
    pub compact: Histogram<Duration>,

    #[metrics(buckets = Buckets::exponential(1.0..=1000.0, 2.0))]
    pub compact_batch_size: Histogram<u64>,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "preimages_rocks_db")]
pub struct PreimagesRocksDBMetrics {
    #[metrics(unit = Unit::Seconds, labels = ["stage"], buckets = LATENCIES_FAST)]
    pub get_latency: LabeledFamily<&'static str, Histogram<Duration>>,

    #[metrics(unit = Unit::Seconds, labels = ["stage"], buckets = LATENCIES_FAST)]
    pub set_latency: LabeledFamily<&'static str, Histogram<Duration>>,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "block_replay_storage")]
pub struct BlockReplayRocksDBMetrics {
    #[metrics(unit = Unit::Seconds, buckets = LATENCIES_FAST)]
    pub get_latency: Histogram<Duration>,

    #[metrics(unit = Unit::Seconds, buckets = LATENCIES_FAST)]
    pub set_latency: Histogram<Duration>,
}

#[vise::register]
pub(crate) static EXECUTION_METRICS: vise::Global<ExecutionMetrics> = vise::Global::new();
#[vise::register]
pub(crate) static STORAGE_VIEW_METRICS: vise::Global<StorageViewMetrics> = vise::Global::new();
#[vise::register]
pub(crate) static STORAGE_MAP_ROCKS_DB_METRICS: vise::Global<StorageMapRocksDBMetrics> = vise::Global::new();
#[vise::register]
pub(crate) static PREIMAGES_ROCKS_DB_METRICS: vise::Global<PreimagesRocksDBMetrics> = vise::Global::new();
#[vise::register]
pub(crate) static BLOCK_REPLAY_ROCKS_DB_METRICS: vise::Global<BlockReplayRocksDBMetrics> = vise::Global::new();
