use std::path::PathBuf;

use k256::ecdsa::SigningKey;
use url::Url;
use zksync_env_config::FromEnv;
use zksync_types::tee_types::TeeType;

pub(crate) struct TeeProverConfig {
    pub signing_key: SigningKey,
    pub attestation_quote_file_path: PathBuf,
    pub tee_type: TeeType,
    pub api_url: Url,
}

impl FromEnv for TeeProverConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            signing_key: std::env::var("TEE_SIGNING_KEY")?.parse()?,
            attestation_quote_file_path: std::env::var("TEE_QUOTE_FILE")?.parse()?,
            tee_type: std::env::var("TEE_TYPE")?.parse()?,
            api_url: std::env::var("TEE_API_URL")?.parse()?,
        })
    }
}
