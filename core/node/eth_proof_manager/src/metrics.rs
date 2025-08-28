use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Family, Metrics};

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
    pub failed_to_send_tx: Family<TxType, Counter>,
    pub validation_result: Family<ValidationResult, Counter>,
    pub proven_batches:  Family<ProvingNetwork, Counter>,
    pub acknowledged_batches:  Family<ProvingNetwork, Counter>,
    pub fallbacked_batches:  Counter<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthProofManagerMetrics> = vise::Global::new();
