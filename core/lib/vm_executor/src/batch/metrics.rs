//! Main batch executor metrics.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};
use zksync_multivm::interface::VmExecutionResultAndLogs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "command", rename_all = "snake_case")]
pub(super) enum ExecutorCommand {
    ExecuteTx,
    #[metrics(name = "start_next_miniblock")]
    StartNextL2Block,
    RollbackLastTx,
    FinishBatch,
}

const GAS_PER_NANOSECOND_BUCKETS: Buckets = Buckets::values(&[
    0.01, 0.03, 0.1, 0.3, 0.5, 0.75, 1., 1.5, 3., 5., 10., 20., 50.,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum TxExecutionStage {
    Execution,
    TxRollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "interaction", rename_all = "snake_case")]
pub(super) enum InteractionType {
    GetValue,
    SetValue,
}

/// Executor-related metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "state_keeper")]
pub(super) struct ExecutorMetrics {
    /// Latency to process a single command sent to the batch executor.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub batch_executor_command_response_time: Family<ExecutorCommand, Histogram<Duration>>,
    #[metrics(buckets = GAS_PER_NANOSECOND_BUCKETS)]
    pub computational_gas_per_nanosecond: Histogram<f64>,
    #[metrics(buckets = GAS_PER_NANOSECOND_BUCKETS)]
    pub failed_tx_gas_limit_per_nanosecond: Histogram<f64>,
    /// Cumulative latency of interacting with the storage when executing a transaction
    /// in the batch executor.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub batch_storage_interaction_duration: Family<InteractionType, Histogram<Duration>>,
}

#[vise::register]
pub(super) static EXECUTOR_METRICS: vise::Global<ExecutorMetrics> = vise::Global::new();

/// Some more executor-related metrics with differing prefix.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_keeper")]
pub(super) struct StateKeeperMetrics {
    /// Time spent by the state keeper on transaction execution.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub tx_execution_time: Family<TxExecutionStage, Histogram<Duration>>,
}

#[vise::register]
pub(super) static KEEPER_METRICS: vise::Global<StateKeeperMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "batch_tip")]
pub(super) struct BatchTipMetrics {
    #[metrics(buckets = Buckets::exponential(60000.0..=80000000.0, 2.0))]
    gas_used: Histogram<usize>,
    #[metrics(buckets = Buckets::exponential(1.0..=60000.0, 2.0))]
    pubdata_published: Histogram<usize>,
    #[metrics(buckets = Buckets::exponential(1.0..=4096.0, 2.0))]
    circuit_statistic: Histogram<usize>,
    #[metrics(buckets = Buckets::exponential(1.0..=4096.0, 2.0))]
    execution_metrics_size: Histogram<usize>,
}

impl BatchTipMetrics {
    pub fn observe(&self, execution_result: &VmExecutionResultAndLogs) {
        self.gas_used
            .observe(execution_result.statistics.gas_used as usize);
        self.pubdata_published
            .observe(execution_result.statistics.pubdata_published as usize);
        self.circuit_statistic
            .observe(execution_result.statistics.circuit_statistic.total());
        self.execution_metrics_size
            .observe(execution_result.get_execution_metrics(None).size());
    }
}

#[vise::register]
pub(super) static BATCH_TIP_METRICS: vise::Global<BatchTipMetrics> = vise::Global::new();
