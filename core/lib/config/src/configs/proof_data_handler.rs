use std::time::Duration;

use serde::Deserialize;
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TeeConfig {
    /// If true, the TEE support is enabled.
    pub tee_support: bool,
    /// All batches before this one are considered to be processed.
    pub first_tee_processed_batch: L1BatchNumber,
    /// Timeout in seconds for retrying TEE proof generation if it fails. Retries continue
    /// indefinitely until successful.
    pub tee_proof_generation_timeout_in_secs: u16,
}

impl Default for TeeConfig {
    fn default() -> Self {
        TeeConfig {
            tee_support: Self::default_tee_support(),
            first_tee_processed_batch: Self::default_first_tee_processed_batch(),
            tee_proof_generation_timeout_in_secs:
                Self::default_tee_proof_generation_timeout_in_secs(),
        }
    }
}

impl TeeConfig {
    pub fn default_tee_support() -> bool {
        false
    }

    pub fn default_first_tee_processed_batch() -> L1BatchNumber {
        L1BatchNumber(0)
    }

    pub fn default_tee_proof_generation_timeout_in_secs() -> u16 {
        600
    }

    pub fn tee_proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.tee_proof_generation_timeout_in_secs.into())
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    pub proof_generation_timeout_in_secs: u16,
    #[serde(skip)]
    // ^ Filled in separately in `Self::from_env()`. We cannot use `serde(flatten)` because it
    // doesn't work with `envy`: https://github.com/softprops/envy/issues/26
    pub tee_config: TeeConfig,
}

impl ProofDataHandlerConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.proof_generation_timeout_in_secs as u64)
    }
}
