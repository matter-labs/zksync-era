use serde::{Deserialize, Serialize};
use url::Url;

use crate::traits::{ReadConfig, SaveConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSecrets {
    pub server_url: String,
    pub prover_url: String,
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
pub struct Secrets {
    pub database: DatabaseSecrets,
    pub l1: L1Secret,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct DatabaseConfig {
    pub base_url: Url,
    pub database_name: String,
}

impl DatabaseConfig {
    pub fn new(base_url: Url, database_name: String) -> Self {
        Self {
            base_url,
            database_name,
        }
    }

    pub fn full_url(&self) -> String {
        format!("{}/{}", self.base_url, self.database_name)
    }
}

#[derive(Debug, Serialize)]
pub struct DatabasesConfig {
    pub server: DatabaseConfig,
    pub prover: DatabaseConfig,
}

impl ReadConfig for Secrets {}
impl SaveConfig for Secrets {}
