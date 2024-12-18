use std::{path::PathBuf, time::Duration};

use secp256k1::SecretKey;
use smart_config::{de::Serde, ConfigSchema, DescribeConfig, DeserializeConfig};
use url::Url;
use zksync_config::configs::PrometheusConfig;
use zksync_types::tee_types::TeeType;

/// Configuration for the TEE prover.
#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub(crate) struct TeeProverConfig {
    /// The private key used to sign the proofs.
    #[config(secret, with = Serde![str])]
    pub signing_key: SecretKey,
    /// The path to the file containing the TEE quote.
    pub attestation_quote_file_path: PathBuf,
    /// Attestation quote file.
    #[config(default_t = TeeType::Sgx, with = Serde![str])]
    pub tee_type: TeeType,
    /// TEE proof data handler API.
    #[config(with = Serde![str])]
    pub api_url: Url,
    /// Number of retries for retriable errors before giving up on recovery (i.e., returning an error
    /// from [`Self::run()`]).
    #[config(default_t = 5)]
    pub max_retries: usize,
    /// Initial back-off interval when retrying recovery on a retriable error. Each subsequent retry interval
    /// will be multiplied by [`Self.retry_backoff_multiplier`].
    #[config(default_t = Duration::from_secs(1))]
    pub initial_retry_backoff: Duration,
    /// Multiplier for the back-off interval when retrying recovery on a retriable error.
    #[config(default_t = 2.0)]
    pub retry_backoff_multiplier: f32,
    /// Maximum back-off interval when retrying recovery on a retriable error.
    #[config(default_t = Duration::from_secs(128))]
    pub max_backoff: Duration,
}

pub(crate) fn config_schema() -> ConfigSchema {
    let mut schema = ConfigSchema::new(&TeeProverConfig::DESCRIPTION, "tee_prover");
    schema
        .insert(&PrometheusConfig::DESCRIPTION, "prometheus")
        .unwrap()
        .push_alias("api.prometheus")
        .unwrap();
    schema
}
