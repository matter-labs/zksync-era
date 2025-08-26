//! Metrics for Ethereum watcher.

use std::time::Duration;

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};


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
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthProofManagerMetrics> = vise::Global::new();
