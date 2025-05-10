use std::time::Duration;

use serde::Deserialize;
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TeeProofDataHandlerConfig {
    pub http_port: u16,
    /// All batches before this one are considered to be processed.
    #[serde(default = "TeeProofDataHandlerConfig::default_first_tee_processed_batch")]
    pub first_processed_batch: L1BatchNumber,
    /// Timeout in seconds for retrying the preparation of input for TEE proof generation if it
    /// previously failed (e.g., due to a transient network issue) or if it was picked by a TEE
    /// prover but the TEE proof was not submitted within that time.
    #[serde(default = "TeeProofDataHandlerConfig::default_tee_proof_generation_timeout_in_secs")]
    pub proof_generation_timeout_in_secs: u16,
    /// Timeout in hours after which a batch will be permanently ignored if repeated retries failed.
    #[serde(
        default = "TeeProofDataHandlerConfig::default_tee_batch_permanently_ignored_timeout_in_hours"
    )]
    pub batch_permanently_ignored_timeout_in_hours: u16,
}

impl TeeProofDataHandlerConfig {
    pub fn default_first_tee_processed_batch() -> L1BatchNumber {
        L1BatchNumber(0)
    }

    pub fn default_tee_proof_generation_timeout_in_secs() -> u16 {
        60
    }

    pub fn default_tee_batch_permanently_ignored_timeout_in_hours() -> u16 {
        10 * 24
    }

    pub fn tee_proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.proof_generation_timeout_in_secs.into())
    }

    pub fn tee_batch_permanently_ignored_timeout(&self) -> Duration {
        Duration::from_secs(3600 * u64::from(self.batch_permanently_ignored_timeout_in_hours))
    }
}
