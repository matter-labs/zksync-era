use std::{path::PathBuf, time::Duration};

use secp256k1::SecretKey;
use serde::Deserialize;
use url::Url;
use zksync_env_config::FromEnv;
use zksync_types::tee_types::TeeType;

/// Configuration for the TEE prover.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TeeProverConfig {
    /// The private key used to sign the proofs.
    pub signing_key: SecretKey,
    /// The path to the file containing the TEE quote.
    pub attestation_quote_file_path: PathBuf,
    /// Attestation quote file.
    pub tee_type: TeeType,
    /// TEE proof data handler API.
    pub api_url: Url,
    /// Number of retries for retriable errors before giving up on recovery (i.e., returning an error
    /// from [`Self::run()`]).
    pub max_retries: usize,
    /// Initial back-off interval when retrying recovery on a retriable error. Each subsequent retry interval
    /// will be multiplied by [`Self.retry_backoff_multiplier`].
    pub initial_retry_backoff_sec: u64,
    /// Multiplier for the back-off interval when retrying recovery on a retriable error.
    pub retry_backoff_multiplier: f32,
    /// Maximum back-off interval when retrying recovery on a retriable error.
    pub max_backoff_sec: u64,
}

impl TeeProverConfig {
    pub fn initial_retry_backoff(&self) -> Duration {
        Duration::from_secs(self.initial_retry_backoff_sec)
    }

    pub fn max_backoff(&self) -> Duration {
        Duration::from_secs(self.max_backoff_sec)
    }
}

impl FromEnv for TeeProverConfig {
    /// Constructs the TEE Prover configuration from environment variables.
    ///
    /// Example usage of environment variables for tests:
    /// ```
    /// export TEE_PROVER_SIGNING_KEY="b50b38c8d396c88728fc032ece558ebda96907a0b1a9340289715eef7bf29deb"
    /// export TEE_PROVER_ATTESTATION_QUOTE_FILE_PATH="/tmp/test"  # run `echo test > /tmp/test` beforehand
    /// export TEE_PROVER_TEE_TYPE="sgx"
    /// export TEE_PROVER_API_URL="http://127.0.0.1:3320"
    /// export TEE_PROVER_MAX_RETRIES=10
    /// export TEE_PROVER_INITIAL_RETRY_BACKOFF_SEC=1
    /// export TEE_PROVER_RETRY_BACKOFF_MULTIPLIER=2.0
    /// export TEE_PROVER_MAX_BACKOFF_SEC=128
    /// ```
    fn from_env() -> anyhow::Result<Self> {
        let config: Self = envy::prefixed("TEE_PROVER_").from_env()?;
        Ok(config)
    }
}
