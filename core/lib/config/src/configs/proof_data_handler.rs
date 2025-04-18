use std::time::Duration;

use serde::Deserialize;
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TeeConfig {
    /// If true, the TEE support is enabled.
    pub tee_support: bool,
    /// All batches before this one are considered to be processed.
    pub first_tee_processed_batch: L1BatchNumber,
    /// Timeout in seconds for retrying the preparation of input for TEE proof generation if it
    /// previously failed (e.g., due to a transient network issue) or if it was picked by a TEE
    /// prover but the TEE proof was not submitted within that time.
    pub tee_proof_generation_timeout_in_secs: u16,
    /// Timeout in hours after which a batch will be permanently ignored if repeated retries failed.
    pub tee_batch_permanently_ignored_timeout_in_hours: u16,
}

impl Default for TeeConfig {
    fn default() -> Self {
        TeeConfig {
            tee_support: Self::default_tee_support(),
            first_tee_processed_batch: Self::default_first_tee_processed_batch(),
            tee_proof_generation_timeout_in_secs:
                Self::default_tee_proof_generation_timeout_in_secs(),
            tee_batch_permanently_ignored_timeout_in_hours:
                Self::default_tee_batch_permanently_ignored_timeout_in_hours(),
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
        60
    }

    pub fn default_tee_batch_permanently_ignored_timeout_in_hours() -> u16 {
        10 * 24
    }

    pub fn tee_proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.tee_proof_generation_timeout_in_secs.into())
    }

    pub fn tee_batch_permanently_ignored_timeout(&self) -> Duration {
        Duration::from_secs(3600 * u64::from(self.tee_batch_permanently_ignored_timeout_in_hours))
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
    #[serde(default)]
    pub gateway_api_url: Option<String>,
    #[serde(default = "ProofDataHandlerConfig::default_proof_fetch_interval_in_secs")]
    pub proof_fetch_interval_in_secs: u16,
    #[serde(default = "ProofDataHandlerConfig::default_proof_gen_data_submit_interval_in_secs")]
    pub proof_gen_data_submit_interval_in_secs: u16,
}

impl ProofDataHandlerConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.proof_generation_timeout_in_secs as u64)
    }

    pub fn proof_fetch_interval(&self) -> Duration {
        Duration::from_secs(self.proof_fetch_interval_in_secs as u64)
    }

    pub fn proof_gen_data_submit_interval(&self) -> Duration {
        Duration::from_secs(self.proof_gen_data_submit_interval_in_secs as u64)
    }

    pub fn default_proof_fetch_interval_in_secs() -> u16 {
        10
    }

    pub fn default_proof_gen_data_submit_interval_in_secs() -> u16 {
        10
    }
}
