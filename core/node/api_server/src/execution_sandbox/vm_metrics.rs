use std::time::Duration;

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LatencyObserver,
    Metrics, Unit,
};
use zksync_types::{
    api::state_override::{BytecodeOverride, OverrideState, StateOverride},
    bytecode::BytecodeMarker,
    H256,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum OverrideKind {
    Code,
    Nonce,
    Balance,
    StorageSlot,
}

impl OverrideKind {
    fn for_method(self, method: &'static str) -> StateOverrideLabels {
        StateOverrideLabels { method, kind: self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct StateOverrideLabels {
    method: &'static str,
    kind: OverrideKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum BytecodeMarkerLabel {
    EraVm,
    DetectedEraVm,
    Evm,
    DetectedEvm,
}

impl BytecodeMarkerLabel {
    fn detected(kind: BytecodeMarker) -> Self {
        match kind {
            BytecodeMarker::EraVm => Self::DetectedEraVm,
            BytecodeMarker::Evm => Self::DetectedEvm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct BytecodeOverrideLabels {
    method: &'static str,
    kind: BytecodeMarkerLabel,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_web3")]
pub(crate) struct SandboxMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub(super) sandbox: Family<SandboxStage, Histogram<Duration>>,
    /// Latency of interrupted VM executions. VM execution is interrupted if the future containing it is dropped
    /// (e.g., on a client-side or server-side request timeout).
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub(crate) sandbox_interrupted_execution_latency: Histogram<Duration>,
    #[metrics(buckets = Buckets::linear(0.0..=2_000.0, 200.0))]
    pub(super) sandbox_execution_permits: Histogram<usize>,
    #[metrics(buckets = Buckets::LATENCIES)]
    submit_tx: Family<SubmitTxStage, Histogram<Duration>>,

    /// Number of iterations necessary to estimate gas for a transaction.
    #[metrics(buckets = Buckets::linear(0.0..=30.0, 3.0))]
    pub estimate_gas_binary_search_iterations: Histogram<usize>,
    /// Relative difference between the unscaled final gas estimate and the optimized lower bound. Positive if the lower bound
    /// is (as expected) lower than the final gas estimate.
    #[metrics(buckets = Buckets::linear(-0.05..=0.15, 0.01))]
    pub estimate_gas_lower_bound_relative_diff: Histogram<f64>,
    /// Relative difference between the optimistic gas limit and the unscaled final gas estimate. Positive if the optimistic gas limit
    /// is (as expected) greater than the final gas estimate.
    #[metrics(buckets = Buckets::linear(-0.05..=0.15, 0.01))]
    pub estimate_gas_optimistic_gas_limit_relative_diff: Histogram<f64>,
    /// Statistics on state overrides.
    state_overrides: Family<StateOverrideLabels, Counter>,
    /// Statistics on bytecode kinds supplied in overrides.
    bytecode_overrides: Family<BytecodeOverrideLabels, Counter>,
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

    pub fn observe_override_metrics(&self, method: &'static str, state_overrides: &StateOverride) {
        for (_, account_override) in state_overrides.iter() {
            if let Some(code_override) = &account_override.code {
                self.state_overrides[&OverrideKind::Code.for_method(method)].inc();

                let bytecode_kind = match code_override {
                    BytecodeOverride::Evm(_) => BytecodeMarkerLabel::Evm,
                    BytecodeOverride::EraVm(_) => BytecodeMarkerLabel::EraVm,
                    BytecodeOverride::Unspecified(bytes) => {
                        // Bytecode kind detection is very cheap, so it's permissible to do it here
                        let kind = BytecodeMarker::detect(&bytes.0);
                        BytecodeMarkerLabel::detected(kind)
                    }
                };
                let labels = BytecodeOverrideLabels {
                    method,
                    kind: bytecode_kind,
                };
                self.bytecode_overrides[&labels].inc();
            }
            if account_override.nonce.is_some() {
                self.state_overrides[&OverrideKind::Nonce.for_method(method)].inc();
            }
            if account_override.balance.is_some() {
                self.state_overrides[&OverrideKind::Balance.for_method(method)].inc();
            }
            if let Some(state) = &account_override.state {
                let slot_count = match state {
                    OverrideState::State(slots) | OverrideState::StateDiff(slots) => slots.len(),
                };
                self.state_overrides[&OverrideKind::StorageSlot.for_method(method)]
                    .inc_by(slot_count as u64);
            }
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
