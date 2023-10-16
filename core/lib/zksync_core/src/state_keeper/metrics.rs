//! General-purpose state keeper metrics.

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LatencyObserver,
    Metrics,
};

use std::{
    sync::{Mutex, Weak},
    time::Duration,
};

use zksync_mempool::MempoolStore;

use super::seal_criteria::SealResolution;
use crate::metrics::InteractionType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum TxExecutionStage {
    Execution,
    TxRollback,
    #[metrics(name = "dryrun_make_snapshot")]
    DryRunMakeSnapshot,
    #[metrics(name = "dryrun_execute_block_tip")]
    DryRunExecuteBlockTip,
    #[metrics(name = "dryrun_get_execution_metrics")]
    DryRunGetExecutionMetrics,
    #[metrics(name = "dryrun_rollback_to_the_latest_snapshot")]
    DryRunRollbackToLatestSnapshot,
    #[metrics(name = "dryrun_rollback")]
    DryRunRollback,
}

/// General-purpose state keeper metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_keeper")]
pub(crate) struct StateKeeperMetrics {
    /// Latency to synchronize the mempool with Postgres.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub mempool_sync: Histogram<Duration>,
    /// Latency of the state keeper waiting for a transaction.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub waiting_for_tx: Histogram<Duration>,
    /// Latency of the state keeper getting a transaction from the mempool.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub get_tx_from_mempool: Histogram<Duration>,
    /// Number of transactions rejected by the state keeper.
    pub rejected_transactions: Counter,
    /// Time spent waiting for the hash of a previous L1 batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub wait_for_prev_hash_time: Histogram<Duration>,
    /// Time spent waiting for the header of a previous miniblock.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub load_previous_miniblock_header: Histogram<Duration>,
    /// Time spent by the state keeper on transaction execution.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub tx_execution_time: Family<TxExecutionStage, Histogram<Duration>>,
    /// Number of times gas price was reported as too high.
    pub gas_price_too_high: Counter,
}

#[vise::register]
pub(crate) static KEEPER_METRICS: vise::Global<StateKeeperMetrics> = vise::Global::new();

/// State keeper-related gauges exposed via a collector.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_keeper")]
pub(super) struct StateKeeperGauges {
    /// Current number of L1 transactions in the mempool.
    mempool_l1_size: Gauge<usize>,
    /// Current number of L2 transactions in the mempool.
    mempool_l2_size: Gauge<u64>,
    /// Current size of the L2 priority queue.
    l2_priority_queue_size: Gauge<usize>,
}

impl StateKeeperGauges {
    pub(super) fn register(pool_ref: Weak<Mutex<MempoolStore>>) {
        #[vise::register]
        static COLLECTOR: vise::Collector<Option<StateKeeperGauges>> = vise::Collector::new();

        let res = COLLECTOR.before_scrape(move || {
            pool_ref.upgrade().map(|pool| {
                let stats = pool.lock().expect("failed to acquire mempool lock").stats();
                drop(pool); // Don't prevent the pool to be dropped

                let gauges = StateKeeperGauges::default();
                gauges.mempool_l1_size.set(stats.l1_transaction_count);
                gauges.mempool_l2_size.set(stats.l2_transaction_count);
                gauges
                    .l2_priority_queue_size
                    .set(stats.l2_priority_queue_size);
                gauges
            })
        });
        if res.is_err() {
            tracing::warn!(
                "Mempool registered for metrics multiple times; this is a logical error"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum SealResolutionLabel {
    NoSeal,
    IncludeAndSeal,
    ExcludeAndSeal,
    Unexecutable,
}

impl From<&SealResolution> for SealResolutionLabel {
    fn from(resolution: &SealResolution) -> Self {
        match resolution {
            SealResolution::NoSeal => Self::NoSeal,
            SealResolution::IncludeAndSeal => Self::IncludeAndSeal,
            SealResolution::ExcludeAndSeal => Self::ExcludeAndSeal,
            SealResolution::Unexecutable(_) => Self::Unexecutable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct TxAggregationLabels {
    criterion: &'static str,
    seal_resolution: Option<SealResolutionLabel>,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_tx_aggregation")]
pub(super) struct TxAggregationMetrics {
    reason: Family<TxAggregationLabels, Counter>,
}

impl TxAggregationMetrics {
    pub fn inc(&self, criterion: &'static str, resolution: &SealResolution) {
        let labels = TxAggregationLabels {
            criterion,
            seal_resolution: Some(resolution.into()),
        };
        self.reason[&labels].inc();
    }

    pub fn inc_criterion(&self, criterion: &'static str) {
        let labels = TxAggregationLabels {
            criterion,
            seal_resolution: None,
        };
        self.reason[&labels].inc();
    }
}

#[vise::register]
pub(super) static AGGREGATION_METRICS: vise::Global<TxAggregationMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum L1BatchSealStage {
    VmFinalization,
    FictiveMiniblock,
    LogDeduplication,
    InsertL1BatchHeader,
    SetL1BatchNumberForMiniblocks,
    MarkTxsAsExecutedInL1Batch,
    InsertProtectiveReads,
    FilterWrittenSlots,
    InsertInitialWrites,
    CommitL1Batch,
    ExternalNodeStoreTransactions,
}

/// Buckets for positive integer, not-so-large values (e.g., initial writes count).
const COUNT_BUCKETS: Buckets = Buckets::values(&[
    10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0,
]);
/// Buckets for sealing deltas for L1 batches (in seconds). The expected delta is ~1 minute.
const L1_BATCH_SEAL_DELTA_BUCKETS: Buckets = Buckets::values(&[
    0.1, 0.5, 1.0, 5.0, 10.0, 20.0, 30.0, 40.0, 60.0, 90.0, 120.0, 180.0, 240.0, 300.0,
]);

/// Metrics related to L1 batch sealing.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_keeper_l1_batch")]
pub(crate) struct L1BatchMetrics {
    /// Delta between sealing consecutive L1 batches.
    #[metrics(buckets = L1_BATCH_SEAL_DELTA_BUCKETS)]
    pub seal_delta: Histogram<Duration>,
    /// Number of initial writes in a single L1 batch.
    #[metrics(buckets = COUNT_BUCKETS)]
    pub initial_writes: Histogram<usize>,
    /// Number of repeated writes in a single L1 batch.
    #[metrics(buckets = COUNT_BUCKETS)]
    pub repeated_writes: Histogram<usize>,
    /// Number of transactions in a single L1 batch.
    #[metrics(buckets = COUNT_BUCKETS)]
    pub transactions_in_l1_batch: Histogram<usize>,
    /// Total latency of sealing an L1 batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub sealed_time: Histogram<Duration>,
    /// Latency of sealing an L1 batch split by the stage.
    #[metrics(buckets = Buckets::LATENCIES)]
    sealed_time_stage: Family<L1BatchSealStage, Histogram<Duration>>,
    /// Number of entities stored in Postgres during a specific stage of sealing an L1 batch.
    #[metrics(buckets = COUNT_BUCKETS)]
    sealed_entity_count: Family<L1BatchSealStage, Histogram<usize>>,
    /// Latency of sealing an L1 batch split by the stage and divided by the number of entiries
    /// stored in the stage.
    #[metrics(buckets = Buckets::LATENCIES)]
    sealed_entity_per_unit: Family<L1BatchSealStage, Histogram<Duration>>,
}

impl L1BatchMetrics {
    pub(super) fn start(&self, stage: L1BatchSealStage) -> SealProgress<'_> {
        SealProgress {
            target: "L1 batch",
            stage_name: format!("{stage:?}"),
            latency: self.sealed_time_stage[&stage].start(),
            entity_count: &self.sealed_entity_count[&stage],
            latency_per_unit: &self.sealed_entity_per_unit[&stage],
        }
    }

    pub(crate) fn start_storing_on_en(&self) -> LatencyObserver<'_> {
        self.sealed_time_stage[&L1BatchSealStage::ExternalNodeStoreTransactions].start()
    }
}

#[vise::register]
pub(crate) static L1_BATCH_METRICS: vise::Global<L1BatchMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum MiniblockQueueStage {
    Submit,
    WaitForAllCommands,
    NextCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(super) enum MiniblockSealStage {
    InsertMiniblockHeader,
    MarkTransactionsInMiniblock,
    InsertStorageLogs,
    ApplyStorageLogs,
    InsertFactoryDeps,
    ExtractContractsDeployed,
    ExtractAddedTokens,
    InsertTokens,
    ExtractEvents,
    InsertEvents,
    ExtractL2ToL1Logs,
    InsertL2ToL1Logs,
    CommitMiniblock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct MiniblockSealLabels {
    stage: MiniblockSealStage,
    is_fictive: &'static str,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_keeper_miniblock")]
pub(super) struct MiniblockMetrics {
    /// Delta between sealing consecutive miniblocks.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub seal_delta: Histogram<Duration>,
    /// Current capacity of the seal queue for miniblocks.
    pub seal_queue_capacity: Gauge<usize>,
    /// Latency of a certain operation concerning the seal queue for miniblocks.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub seal_queue_latency: Family<MiniblockQueueStage, Histogram<Duration>>,
    /// Number of transactions in a single miniblock.
    #[metrics(buckets = Buckets::linear(0.0..=50.0, 5.0))]
    pub transactions_in_miniblock: Histogram<usize>,
    /// Total latency of sealing a miniblock.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub sealed_time: Histogram<Duration>,
    /// Latency of sealing a miniblock split by the stage.
    #[metrics(buckets = Buckets::LATENCIES)]
    sealed_time_stage: Family<MiniblockSealLabels, Histogram<Duration>>,
    /// Number of entities stored in Postgres during a specific stage of sealing a miniblock.
    #[metrics(buckets = COUNT_BUCKETS)]
    sealed_entity_count: Family<MiniblockSealLabels, Histogram<usize>>,
    /// Latency of sealing a miniblock split by the stage and divided by the number of entiries
    /// stored in the stage.
    #[metrics(buckets = Buckets::LATENCIES)]
    sealed_entity_per_unit: Family<MiniblockSealLabels, Histogram<Duration>>,
}

impl MiniblockMetrics {
    pub(super) fn start(&self, stage: MiniblockSealStage, is_fictive: bool) -> SealProgress<'_> {
        let labels = MiniblockSealLabels {
            stage,
            is_fictive: if is_fictive { "true" } else { "false" },
        };
        SealProgress {
            target: "Miniblock",
            stage_name: format!("{stage:?}"),
            latency: self.sealed_time_stage[&labels].start(),
            entity_count: &self.sealed_entity_count[&labels],
            latency_per_unit: &self.sealed_entity_per_unit[&labels],
        }
    }
}

#[vise::register]
pub(super) static MINIBLOCK_METRICS: vise::Global<MiniblockMetrics> = vise::Global::new();

/// Tracking progress of L1 batch or miniblock sealing.
#[must_use = "Progress must be `observe()`d"]
#[derive(Debug)]
pub(super) struct SealProgress<'a> {
    target: &'static str,
    stage_name: String,
    latency: LatencyObserver<'a>,
    entity_count: &'a Histogram<usize>,
    latency_per_unit: &'a Histogram<Duration>,
}

impl SealProgress<'_> {
    pub fn observe(self, count: impl Into<Option<usize>>) {
        const MIN_STAGE_DURATION_TO_REPORT: Duration = Duration::from_millis(10);

        let count = count.into();
        let elapsed = self.latency.observe();
        if elapsed > MIN_STAGE_DURATION_TO_REPORT {
            let target = self.target;
            let stage = &self.stage_name;
            tracing::debug!(
                "{target} execution stage {stage} took {elapsed:?} with count {count:?}"
            );
        }

        if let Some(count) = count {
            self.entity_count.observe(count);
            if count > 0 {
                self.latency_per_unit.observe(elapsed.div_f64(count as f64));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "command", rename_all = "snake_case")]
pub(super) enum ExecutorCommand {
    ExecuteTx,
    StartNextMiniblock,
    RollbackLastTx,
    FinishBatch,
}

const GAS_PER_NANOSECOND_BUCKETS: Buckets = Buckets::values(&[
    0.01, 0.03, 0.1, 0.3, 0.5, 0.75, 1., 1.5, 3., 5., 10., 20., 50.,
]);

/// Executor-related state keeper metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "state_keeper")]
pub(super) struct ExecutorMetrics {
    /// Latency to process a single command sent to the batch executor.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub batch_executor_command_response_time: Family<ExecutorCommand, Histogram<Duration>>,
    /// Cumulative latency of interacting with the storage when executing a transaction
    /// in the batch executor.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub batch_storage_interaction_duration: Family<InteractionType, Histogram<Duration>>,
    #[metrics(buckets = GAS_PER_NANOSECOND_BUCKETS)]
    pub computational_gas_per_nanosecond: Histogram<f64>,
    #[metrics(buckets = GAS_PER_NANOSECOND_BUCKETS)]
    pub failed_tx_gas_limit_per_nanosecond: Histogram<f64>,
}

#[vise::register]
pub(super) static EXECUTOR_METRICS: vise::Global<ExecutorMetrics> = vise::Global::new();
