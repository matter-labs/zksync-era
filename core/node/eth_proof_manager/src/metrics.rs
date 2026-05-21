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
    #[metrics(labels = ["submitter_address"])]
    pub submitter_address: LabeledFamily<&'static str, Gauge, 1>,
    #[metrics(labels = ["contract_address"])]
    pub contract_address: LabeledFamily<&'static str, Gauge, 1>,
    pub submitter_balance: Gauge<f64>,
    /// Free USDC balance of the ProofManager contract: the on-chain USDC
    /// balance minus the `owedReward` already accrued to each proving network.
    /// Approximates the contract's own `_can_accept_request` view, but does
    /// *not* subtract `potentialFutureReward` or `heapObligations` because
    /// those fields are `internal` on the contract and have no public getter,
    /// so this gauge reads higher than the true free amount when requests are
    /// in flight. Expressed in raw token units (USDC has 6 decimals).
    pub proof_manager_free_usdc: Gauge<f64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthProofManagerMetrics> = vise::Global::new();
