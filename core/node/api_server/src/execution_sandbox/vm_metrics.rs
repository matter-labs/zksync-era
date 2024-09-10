use std::time::Duration;

use vise::{
    Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LatencyObserver, Metrics,
};
use zksync_multivm::{
    interface::{TransactionExecutionMetrics, VmEvent, VmExecutionResultAndLogs},
    utils::StorageWritesDeduplicator,
};
use zksync_types::H256;
use zksync_utils::bytecode::bytecode_len_in_bytes;

use crate::utils::ReportFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum SandboxStage {
    VmConcurrencyLimiterAcquire,
    Initialization,
    ValidateInSandbox,
    Validation,
    Execution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum SubmitTxStage {
    #[metrics(name = "1_validate")]
    Validate,
    #[metrics(name = "2_dry_run")]
    DryRun,
    #[metrics(name = "3_verify_execute")]
    VerifyExecute,
    #[metrics(name = "4_tx_proxy")]
    TxProxy,
    #[metrics(name = "4_db_insert")]
    DbInsert,
}

#[must_use = "should be `observe()`d"]
#[derive(Debug)]
pub(crate) struct SubmitTxLatencyObserver<'a> {
    inner: Option<LatencyObserver<'a>>,
    tx_hash: H256,
    stage: SubmitTxStage,
}

impl SubmitTxLatencyObserver<'_> {
    pub fn set_stage(&mut self, stage: SubmitTxStage) {
        self.stage = stage;
    }

    pub fn observe(mut self) {
        static FILTER: ReportFilter = report_filter!(Duration::from_secs(10));
        const MIN_LOGGED_LATENCY: Duration = Duration::from_secs(1);

        let latency = self.inner.take().unwrap().observe();
        // ^ `unwrap()` is safe: `LatencyObserver` is only taken out in this method.
        if latency > MIN_LOGGED_LATENCY && FILTER.should_report() {
            tracing::info!(
                "Transaction {:?} submission stage {:?} has high latency: {latency:?}",
                self.tx_hash,
                self.stage
            );
        }
    }
}

impl Drop for SubmitTxLatencyObserver<'_> {
    fn drop(&mut self) {
        static FILTER: ReportFilter = report_filter!(Duration::from_secs(10));

        if self.inner.is_some() && FILTER.should_report() {
            tracing::info!(
                "Transaction {:?} submission was dropped at stage {:?} due to error or client disconnecting",
                self.tx_hash,
                self.stage
            );
        }
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_web3")]
pub(crate) struct SandboxMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub(super) sandbox: Family<SandboxStage, Histogram<Duration>>,
    #[metrics(buckets = Buckets::linear(0.0..=2_000.0, 200.0))]
    pub(super) sandbox_execution_permits: Histogram<usize>,
    #[metrics(buckets = Buckets::LATENCIES)]
    submit_tx: Family<SubmitTxStage, Histogram<Duration>>,
    #[metrics(buckets = Buckets::linear(0.0..=30.0, 3.0))]
    pub estimate_gas_binary_search_iterations: Histogram<usize>,
}

impl SandboxMetrics {
    pub fn start_tx_submit_stage(
        &self,
        tx_hash: H256,
        stage: SubmitTxStage,
    ) -> SubmitTxLatencyObserver<'_> {
        SubmitTxLatencyObserver {
            inner: Some(self.submit_tx[&stage].start()),
            tx_hash,
            stage,
        }
    }
}

#[vise::register]
pub(crate) static SANDBOX_METRICS: vise::Global<SandboxMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_execution")]
pub(super) struct ExecutionMetrics {
    pub tokens_amount: Gauge<usize>,
    pub trusted_address_slots_amount: Gauge<usize>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub get_validation_params: Histogram<Duration>,
}

#[vise::register]
pub(super) static EXECUTION_METRICS: vise::Global<ExecutionMetrics> = vise::Global::new();

pub(super) fn collect_tx_execution_metrics(
    contracts_deployed: u16,
    result: &VmExecutionResultAndLogs,
) -> TransactionExecutionMetrics {
    let writes_metrics = StorageWritesDeduplicator::apply_on_empty_state(&result.logs.storage_logs);
    let event_topics = result
        .logs
        .events
        .iter()
        .map(|event| event.indexed_topics.len() as u16)
        .sum();
    let l2_l1_long_messages = VmEvent::extract_long_l2_to_l1_messages(&result.logs.events)
        .iter()
        .map(|event| event.len())
        .sum();
    let published_bytecode_bytes = VmEvent::extract_published_bytecodes(&result.logs.events)
        .iter()
        .map(|bytecode_hash| bytecode_len_in_bytes(*bytecode_hash))
        .sum();

    TransactionExecutionMetrics {
        initial_storage_writes: writes_metrics.initial_storage_writes,
        repeated_storage_writes: writes_metrics.repeated_storage_writes,
        gas_used: result.statistics.gas_used as usize,
        gas_remaining: result.statistics.gas_remaining,
        event_topics,
        published_bytecode_bytes,
        l2_l1_long_messages,
        l2_l1_logs: result.logs.total_l2_to_l1_logs_count(),
        contracts_used: result.statistics.contracts_used,
        contracts_deployed,
        vm_events: result.logs.events.len(),
        storage_logs: result.logs.storage_logs.len(),
        total_log_queries: result.statistics.total_log_queries,
        cycles_used: result.statistics.cycles_used,
        computational_gas_used: result.statistics.computational_gas_used,
        total_updated_values_size: writes_metrics.total_updated_values_size,
        pubdata_published: result.statistics.pubdata_published,
        circuit_statistic: result.statistics.circuit_statistic,
    }
}
