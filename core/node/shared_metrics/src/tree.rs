//! Shared metrics for Merkle tree updater.

use std::time::{Duration, Instant, SystemTime};

use vise::{
    Buckets, DurationAsSecs, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Info,
    LatencyObserver, Metrics, Unit,
};
use zksync_types::block::L1BatchStatistics;

use crate::{BlockStage, APP_METRICS};

#[derive(Debug, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub enum ModeLabel {
    Full,
    Lightweight,
}

#[derive(Debug, EncodeLabelSet)]
pub struct ConfigLabels {
    pub mode: ModeLabel,
    #[metrics(unit = Unit::Seconds)]
    pub delay_interval: DurationAsSecs,
    pub max_l1_batches_per_iter: usize,
    pub multi_get_chunk_size: usize,
    #[metrics(unit = Unit::Bytes)]
    pub block_cache_capacity: usize,
    #[metrics(unit = Unit::Bytes)]
    pub memtable_capacity: usize,
    #[metrics(unit = Unit::Seconds)]
    pub stalled_writes_timeout: DurationAsSecs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub enum TreeUpdateStage {
    LoadChanges,
    Compute,
    SavePostgres,
    SaveRocksdb,
    SaveGcs,
}

/// Sub-stages of [`TreeUpdateStage::LoadChanges`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub enum LoadChangesStage {
    LoadL1BatchHeader,
    LoadProtectiveReads,
    LoadTouchedSlots,
    LoadLeafIndices,
    LoadTreeWrites,
}

/// Latency metric for a certain stage of the tree update.
#[derive(Debug)]
#[must_use = "Tree latency should be `report`ed"]
pub struct UpdateTreeLatency<'a, const COUNT: bool> {
    stage: String,
    latency: LatencyObserver<'a>,
    counter: Option<&'a Histogram<usize>>,
}

impl<const COUNT: bool> UpdateTreeLatency<'_, COUNT> {
    pub fn observe(self) {
        self.observe_inner(None);
    }

    fn observe_inner(self, record_count: Option<usize>) {
        let stage = &self.stage;
        let elapsed = self.latency.observe();
        if let Some(record_count) = record_count {
            tracing::debug!(
                "Metadata calculator stage `{stage}` with {record_count} records completed in {elapsed:?}"
            );
            self.counter.unwrap().observe(record_count);
            // ^ `unwrap()` is safe by construction
        } else {
            tracing::debug!("Metadata calculator stage `{stage}` completed in {elapsed:?}");
        }
    }
}

impl UpdateTreeLatency<'_, true> {
    pub fn observe_with_count(self, count: usize) {
        self.observe_inner(Some(count));
    }
}

const COUNTS_BUCKETS: Buckets = Buckets::values(&[
    100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0, 100_000.0,
]);
/// Buckets for `update_tree_per_log_latency` (from 1us to 10ms). The expected order of latencies
/// should be ~100us / log for large trees.
const LATENCIES_PER_LOG: Buckets = Buckets::values(&[
    1e-6, 2.5e-6, 5e-6, 1e-5, 2.5e-5, 5e-5, 1e-4, 2.5e-4, 5e-4, 1e-3, 5e-3, 1e-2,
]);

/// Metrics for the metadata calculator.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_metadata_calculator")]
pub struct MetadataCalculatorMetrics {
    /// Merkle tree configuration.
    pub info: Info<ConfigLabels>,
    /// Lag between the number of L1 batches processed in the Merkle tree and stored in Postgres.
    /// The lag can only be positive if Postgres was restored from a backup truncating some
    /// of the batches already processed by the tree.
    pub backup_lag: Gauge<u64>,
    /// Number of zero values that need to be checked for L1 batch of the initial write in the process
    /// of updating the Merkle tree.
    #[metrics(buckets = COUNTS_BUCKETS)]
    pub load_changes_zero_values: Histogram<usize>,
    /// Total latency of updating the Merkle tree.
    #[metrics(buckets = Buckets::LATENCIES)]
    update_tree_latency: Histogram<Duration>,
    /// Latency of updating the Merkle tree divided by the number of updated entries.
    #[metrics(buckets = LATENCIES_PER_LOG)]
    update_tree_per_log_latency: Histogram<Duration>,
    /// Number of entries updated in the Merkle tree for a single L1 batch.
    #[metrics(buckets = COUNTS_BUCKETS)]
    log_batch: Histogram<usize>,
    /// Number of L1 batches applied to the Merkle tree in a single iteration.
    #[metrics(buckets = Buckets::linear(1.0..=20.0, 1.0))]
    blocks_batch: Histogram<usize>,
    /// Latency of updating the Merkle tree per stage.
    #[metrics(buckets = Buckets::LATENCIES)]
    update_tree_latency_stage: Family<TreeUpdateStage, Histogram<Duration>>,
    /// Latency of loading changes from Postgres split into stages.
    #[metrics(buckets = Buckets::LATENCIES)]
    load_changes_latency: Family<LoadChangesStage, Histogram<Duration>>,
    /// Number of changes loaded from Postgres in a specific loading stage.
    #[metrics(buckets = COUNTS_BUCKETS)]
    load_changes_count: Family<LoadChangesStage, Histogram<usize>>,
}

impl MetadataCalculatorMetrics {
    pub fn start_stage(&self, stage: TreeUpdateStage) -> UpdateTreeLatency<'_, false> {
        UpdateTreeLatency {
            stage: format!("{stage:?}"),
            latency: self.update_tree_latency_stage[&stage].start(),
            counter: None,
        }
    }

    pub fn start_load_stage(&self, stage: LoadChangesStage) -> UpdateTreeLatency<'_, true> {
        UpdateTreeLatency {
            stage: format!("{stage:?}"),
            latency: self.load_changes_latency[&stage].start(),
            counter: Some(&self.load_changes_count[&stage]),
        }
    }
}

#[vise::register]
pub static METRICS: vise::Global<MetadataCalculatorMetrics> = vise::Global::new();

pub fn update_tree_metrics(batch_stats: &[L1BatchStatistics], total_logs: usize, start: Instant) {
    let (Some(first_header), Some(last_header)) = (batch_stats.first(), batch_stats.last()) else {
        return;
    };

    let elapsed = start.elapsed();
    METRICS.update_tree_latency.observe(elapsed);
    if total_logs > 0 {
        METRICS
            .update_tree_per_log_latency
            .observe(elapsed.div_f32(total_logs as f32));
    }

    let total_tx: usize = batch_stats
        .iter()
        .map(|stats| stats.l1_tx_count as usize + stats.l2_tx_count as usize)
        .sum();
    let total_l1_tx_count: u64 = batch_stats
        .iter()
        .map(|stats| u64::from(stats.l1_tx_count))
        .sum();
    APP_METRICS.processed_txs[&BlockStage::Tree.into()].inc_by(total_tx as u64);
    APP_METRICS.processed_l1_txs[&BlockStage::Tree.into()].inc_by(total_l1_tx_count);
    METRICS.log_batch.observe(total_logs);
    METRICS.blocks_batch.observe(batch_stats.len());

    let first_batch_number = first_header.number.0;
    let last_batch_number = last_header.number.0;
    tracing::info!(
        "L1 batches #{:?} processed in tree",
        first_batch_number..=last_batch_number
    );
    APP_METRICS.block_number[&BlockStage::Tree].set(last_batch_number.into());

    let duration_since_epoch = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("incorrect system time");
    let latency = duration_since_epoch.saturating_sub(Duration::from_secs(first_header.timestamp));
    APP_METRICS.block_latency[&BlockStage::Tree].observe(latency);
}
