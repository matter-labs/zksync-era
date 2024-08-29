use std::{path::PathBuf, time::Duration};

use secp256k1::{PublicKey, Secp256k1, SecretKey};
use url::Url;
use zksync_env_config::FromEnv;
use zksync_types::tee_types::TeeType;

/// Configuration for the TEE prover.
#[derive(Debug, Clone)]
pub(crate) struct TeeProverConfig {
    /// The private key used to sign the proofs.
    pub signing_key: SecretKey,
    /// The public key used to verify the proofs.
    pub public_key: PublicKey,
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
    pub initial_retry_backoff: Duration,
    /// Multiplier for the back-off interval when retrying recovery on a retriable error.
    pub retry_backoff_multiplier: f32,
    /// Maximum back-off interval when retrying recovery on a retriable error.
    pub max_backoff: Duration,
}

impl FromEnv for TeeProverConfig {
    /// Constructs the TEE Prover configuration from environment variables.
    ///
    /// Example usage of environment variables for tests:
    /// ```
    /// export TEE_PROVER_SIGNING_KEY="b50b38c8d396c88728fc032ece558ebda96907a0b1a9340289715eef7bf29deb"
    /// export TEE_PROVER_QUOTE_FILE="/tmp/test"  # run `echo test > /tmp/test` beforehand
    /// export TEE_PROVER_TYPE="sgx"
    /// export TEE_PROVER_API_URL="http://127.0.0.1:3320"
    /// export TEE_PROVER_MAX_RETRIES=10
    /// export TEE_PROVER_INITIAL_RETRY_BACKOFF_SECONDS=1
    /// export TEE_PROVER_RETRY_BACKOFF_MULTIPLIER=2.0
    /// export TEE_PROVER_MAX_BACKOFF_SECONDS=128
    /// ```
    fn from_env() -> anyhow::Result<Self> {
        let signing_key = std::env::var("TEE_PROVER_SIGNING_KEY")?.parse()?;
        Ok(Self {
            signing_key,
            public_key: signing_key.public_key(&Secp256k1::new()),
            attestation_quote_file_path: std::env::var("TEE_PROVER_QUOTE_FILE")?.parse()?,
            tee_type: std::env::var("TEE_PROVER_TYPE")?.parse()?,
            api_url: std::env::var("TEE_PROVER_API_URL")?.parse()?,
            max_retries: std::env::var("TEE_PROVER_MAX_RETRIES")?.parse()?,
            initial_retry_backoff: Duration::from_secs(
                std::env::var("TEE_PROVER_INITIAL_RETRY_BACKOFF_SECONDS")
                    .unwrap_or_else(|_| "1".to_string())
                    .parse()?,
            ),
            retry_backoff_multiplier: std::env::var("TEE_PROVER_RETRY_BACKOFF_MULTIPLIER")
                .unwrap_or("2.0".to_string())
                .parse()?,
            max_backoff: Duration::from_secs(
                std::env::var("TEE_PROVER_MAX_BACKOFF_SECONDS")
                    .unwrap_or_else(|_| "128".to_string())
                    .parse()?,
            ),
        })
    }
}
