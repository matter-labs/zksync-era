use serde::{Deserialize, Serialize};
use url::Url;

use crate::{consts::SECRETS_FILE, traits::FileConfigWithDefaultName};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSecrets {
    pub server_url: Url,
    pub prover_url: Url,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1Secret {
    pub l1_rpc_url: String,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    pub database: DatabaseSecrets,
    pub l1: L1Secret,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl FileConfigWithDefaultName for SecretsConfig {
    const FILE_NAME: &'static str = SECRETS_FILE;
}
