use std::path::PathBuf;

use secp256k1::SecretKey;
use url::Url;
use zksync_env_config::FromEnv;
use zksync_types::tee_types::TeeType;

/// Configuration for the TEE prover.
#[derive(Debug)]
pub(crate) struct TeeProverConfig {
    /// The private key used to sign the proofs.
    pub signing_key: SecretKey,
    /// The path to the file containing the TEE quote.
    pub attestation_quote_file_path: PathBuf,
    /// Attestation quote file.
    pub tee_type: TeeType,
    /// TEE proof data handler API.
    pub api_url: Url,
}

impl FromEnv for TeeProverConfig {
    /// Constructs the TEE Prover configuration from environment variables.
    ///
    /// Example usage of environment variables for tests:
    /// ```
    /// export TEE_SIGNING_KEY="b50b38c8d396c88728fc032ece558ebda96907a0b1a9340289715eef7bf29deb"
    /// export TEE_QUOTE_FILE="/tmp/test"  # run `echo test > /tmp/test` beforehand
    /// export TEE_TYPE="sgx"
    /// export TEE_API_URL="http://127.0.0.1:3320"
    /// ```
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            signing_key: std::env::var("TEE_SIGNING_KEY")?.parse()?,
            attestation_quote_file_path: std::env::var("TEE_QUOTE_FILE")?.parse()?,
            tee_type: std::env::var("TEE_TYPE")?.parse()?,
            api_url: std::env::var("TEE_API_URL")?.parse()?,
        })
    }
}
