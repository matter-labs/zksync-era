use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, LabeledFamily, Metrics};

use crate::types::ProvingNetwork;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "tx_type", rename_all = "snake_case")]
pub(super) enum TxType {
    ProofRequest,
    ValidationResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "validation_result", rename_all = "snake_case")]
pub(super) enum ValidationResult {
    Success,
    Failed,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_eth_proof_manager")]
pub(super) struct EthProofManagerMetrics {
    pub validated_batches: Family<ValidationResult, Counter>,
    pub proven_batches: Family<ProvingNetwork, Gauge<u64>>,
    pub acknowledged_batches: Family<ProvingNetwork, Gauge<u64>>,
    pub fallbacked_batches: Counter<u64>,
    pub reached_max_attempts: Family<TxType, Gauge<u64>>,
    /// Batches closed because the contract already held a validation result for them.
    pub validation_already_recorded: Counter<u64>,
    /// Batches whose on-chain verdict disagrees with the one we were reporting. Non-zero
    /// means a rogue or stale submitter key, or node state diverging from the chain.
    pub validation_verdict_mismatch: Counter<u64>,
    /// Transient handler failures scheduled for retry.
    #[metrics(labels = ["event_type"])]
    pub event_handling_errors: LabeledFamily<&'static str, Counter, 1>,
    /// Events skipped after fatal handler errors.
    #[metrics(labels = ["event_type"])]
    pub skipped_events: LabeledFamily<&'static str, Counter, 1>,
    #[metrics(labels = ["submitter_address"])]
    pub submitter_address: LabeledFamily<&'static str, Gauge, 1>,
    #[metrics(labels = ["contract_address"])]
    pub contract_address: LabeledFamily<&'static str, Gauge, 1>,
    pub submitter_balance: Gauge<f64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthProofManagerMetrics> = vise::Global::new();
