use std::time::Duration;

use smart_config::{de::Serde, metadata::TimeUnit, DescribeConfig, DeserializeConfig};
use zksync_basic_types::L1BatchNumber;

const SECONDS_IN_DAY: u64 = 86_400;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct TeeConfig {
    /// If true, the TEE support is enabled.
    #[config(default)]
    pub tee_support: bool,
    /// All batches before this one are considered to be processed.
    #[config(default, with = Serde![int])]
    pub first_tee_processed_batch: L1BatchNumber,
    /// Timeout in seconds for retrying the preparation of input for TEE proof generation if it
    /// previously failed (e.g., due to a transient network issue) or if it was picked by a TEE
    /// prover but the TEE proof was not submitted within that time.
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub tee_proof_generation_timeout_in_secs: Duration,
    /// Timeout in hours after which a batch will be permanently ignored if repeated retries failed.
    #[config(default_t = Duration::from_secs(10 * SECONDS_IN_DAY), with = TimeUnit::Hours)]
    pub tee_batch_permanently_ignored_timeout_in_hours: Duration,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub proof_generation_timeout_in_secs: Duration,
    #[config(nest)]
    pub tee_config: TeeConfig,
}
