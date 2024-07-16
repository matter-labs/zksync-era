//! Metrics for `MetadataCalculator`.

use std::time::{Duration, Instant};

use vise::{
    Buckets, DurationAsSecs, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Info,
    LatencyObserver, Metrics, Unit,
};
use zksync_config::configs::database::MerkleTreeMode;
use zksync_shared_metrics::{BlockStage, APP_METRICS};
use zksync_types::block::L1BatchHeader;
use zksync_utils::time::seconds_since_epoch;

use super::{MetadataCalculator, MetadataCalculatorConfig};

#[derive(Debug, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum ModeLabel {
    Full,
    Lightweight,
}

impl From<MerkleTreeMode> for ModeLabel {
    fn from(mode: MerkleTreeMode) -> Self {
        match mode {
            MerkleTreeMode::Full => Self::Full,
            MerkleTreeMode::Lightweight => Self::Lightweight,
        }
    }
}

#[derive(Debug, EncodeLabelSet)]
pub(super) struct ConfigLabels {
    mode: ModeLabel,
    #[metrics(unit = Unit::Seconds)]
    delay_interval: DurationAsSecs,
    max_l1_batches_per_iter: usize,
    multi_get_chunk_size: usize,
    #[metrics(unit = Unit::Bytes)]
    block_cache_capacity: usize,
    #[metrics(unit = Unit::Bytes)]
    memtable_capacity: usize,
    #[metrics(unit = Unit::Seconds)]
    stalled_writes_timeout: DurationAsSecs,
}

impl ConfigLabels {
    pub fn new(config: &MetadataCalculatorConfig) -> Self {
        Self {
            mode: config.mode.into(),
            delay_interval: config.delay_interval.into(),
            max_l1_batches_per_iter: config.max_l1_batches_per_iter,
            multi_get_chunk_size: config.multi_get_chunk_size,
            block_cache_capacity: config.block_cache_capacity,
            memtable_capacity: config.memtable_capacity,
            stalled_writes_timeout: config.stalled_writes_timeout.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum TreeUpdateStage {
    LoadChanges,
    Compute,
    SavePostgres,
    SaveRocksdb,
    SaveGcs,
}

/// Sub-stages of [`TreeUpdateStage::LoadChanges`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum LoadChangesStage {
    LoadL1BatchHeader,
    LoadProtectiveReads,
    LoadTouchedSlots,
    LoadLeafIndices,
    LoadTreeWrites,
}

/// Latency metric for a certain stage of the tree update.
#[derive(Debug)]
#[must_use = "Tree latency should be `report`ed"]
pub(super) struct UpdateTreeLatency<'a, const COUNT: bool> {
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
pub(super) struct MetadataCalculatorMetrics {
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
pub(super) static METRICS: vise::Global<MetadataCalculatorMetrics> = vise::Global::new();

impl MetadataCalculator {
    pub(super) fn update_metrics(
        batch_headers: &[L1BatchHeader],
        total_logs: usize,
        start: Instant,
    ) {
        let elapsed = start.elapsed();
        METRICS.update_tree_latency.observe(elapsed);
        if total_logs > 0 {
            METRICS
                .update_tree_per_log_latency
                .observe(elapsed.div_f32(total_logs as f32));
        }

        let total_tx: usize = batch_headers.iter().map(L1BatchHeader::tx_count).sum();
        let total_l1_tx_count: u64 = batch_headers
            .iter()
            .map(|batch| u64::from(batch.l1_tx_count))
            .sum();
        APP_METRICS.processed_txs[&BlockStage::Tree.into()].inc_by(total_tx as u64);
        APP_METRICS.processed_l1_txs[&BlockStage::Tree.into()].inc_by(total_l1_tx_count);
        METRICS.log_batch.observe(total_logs);
        METRICS.blocks_batch.observe(batch_headers.len());

        let first_batch_number = batch_headers.first().unwrap().number.0;
        let last_batch_number = batch_headers.last().unwrap().number.0;
        tracing::info!(
            "L1 batches #{:?} processed in tree",
            first_batch_number..=last_batch_number
        );
        APP_METRICS.block_number[&BlockStage::Tree].set(last_batch_number.into());

        let latency =
            seconds_since_epoch().saturating_sub(batch_headers.first().unwrap().timestamp);
        APP_METRICS.block_latency[&BlockStage::Tree].observe(Duration::from_secs(latency));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum RecoveryStage {
    LoadChunkStarts,
    Finalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum ChunkRecoveryStage {
    AcquireConnection,
    LoadEntries,
    LockTree,
    ExtendTree,
}

/// Metrics for Merkle tree recovery driven by the metadata calculator.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_metadata_calculator_recovery")]
pub(super) struct MetadataCalculatorRecoveryMetrics {
    /// Number of chunks recovered.
    pub recovered_chunk_count: Gauge<u64>,
    /// Latency of a tree recovery stage (not related to the recovery of a particular chunk;
    /// those metrics are tracked in the `chunk_latency` histogram).
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub latency: Family<RecoveryStage, Histogram<Duration>>,
    /// Latency of a chunk recovery stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub chunk_latency: Family<ChunkRecoveryStage, Histogram<Duration>>,
}

#[vise::register]
pub(super) static RECOVERY_METRICS: vise::Global<MetadataCalculatorRecoveryMetrics> =
    vise::Global::new();
