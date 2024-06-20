use serde::{Deserialize, Serialize};
use url::Url;

use common::db::DatabaseConfig;

use crate::{consts::SECRETS_FILE, traits::FileConfigWithDefaultName};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSecrets {
    pub server_url: Url,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prover_url: Option<Url>,
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

impl SecretsConfig {
    pub fn set_databases(&mut self,
                         server_db_config: &DatabaseConfig,
                         prover_db_config: &DatabaseConfig,
    ) {
        self.database.server_url = server_db_config.full_url();
        self.database.prover_url = Some(prover_db_config.full_url());
    }

    pub fn set_l1_rpc_url(
        &mut self,
        l1_rpc_url: String,
    ) {
        self.l1.l1_rpc_url = l1_rpc_url;
    }
}


impl FileConfigWithDefaultName for SecretsConfig {
    const FILE_NAME: &'static str = SECRETS_FILE;
}
