use std::path::PathBuf;

use url::Url;
use xshell::Shell;
use zkstack_cli_common::db::DatabaseConfig;
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles::{node, validator};

use crate::{
    da::AvailSecrets,
    raw::{PatchedConfig, RawConfig},
};

#[derive(Debug)]
pub struct RawConsensusKeys {
    pub validator_public: String,
    pub node_public: String,
    pub validator_secret: String,
    pub node_secret: String,
}

impl RawConsensusKeys {
    pub fn generate() -> Self {
        let validator = validator::SecretKey::generate();
        let node = node::SecretKey::generate();
        
        Self {
            validator_public: validator.public().encode(),
            node_public: node.public().encode(),
            validator_secret: validator.encode(),
            node_secret: node.encode(),
        }
    }
}

#[derive(Debug)]
pub struct SecretsConfig(RawConfig);

impl SecretsConfig {
    pub async fn read(shell: &Shell, path: PathBuf) -> anyhow::Result<Self> {
        RawConfig::read(shell, path).await.map(Self)
    }

    pub fn core_database_url(&self) -> anyhow::Result<Option<Url>> {
        self.0.get_opt("database.server_url")
    }

    pub fn prover_database_url(&self) -> anyhow::Result<Option<Url>> {
        self.0.get_opt("database.prover_url")
    }

    pub fn l1_rpc_url(&self) -> anyhow::Result<String> {
        self.0.get("l1.l1_rpc_url")
    }

    pub fn raw_consensus_node_key(&self) -> anyhow::Result<String> {
        self.0.get("consensus.node_key")
    }

    pub fn patched(self) -> SecretsConfigPatch {
        SecretsConfigPatch(self.0.patched())
    }
}

#[derive(Debug)]
#[must_use = "Must be `save()`d for changes to take effect"]
pub struct SecretsConfigPatch(PatchedConfig);

impl SecretsConfigPatch {
    pub fn empty(shell: &Shell, path: PathBuf) -> Self {
        Self(PatchedConfig::empty(shell, path))
    }

    pub fn set_server_database(&mut self, server_db_config: &DatabaseConfig) -> anyhow::Result<()> {
        self.0.insert(
            "database.server_url",
            server_db_config.full_url().to_string(),
        )
    }

    pub fn set_prover_database(&mut self, prover_db_config: &DatabaseConfig) -> anyhow::Result<()> {
        self.0.insert(
            "database.prover_url",
            prover_db_config.full_url().to_string(),
        )
    }

    pub fn set_l1_rpc_url(&mut self, l1_rpc_url: String) -> anyhow::Result<()> {
        self.0.insert("l1.l1_rpc_url", l1_rpc_url)
    }

    pub fn set_gateway_rpc_url(&mut self, url: String) -> anyhow::Result<()> {
        self.0.insert("l1.gateway_rpc_url", url)
    }

    pub fn set_avail_secrets(&mut self, secrets: &AvailSecrets) -> anyhow::Result<()> {
        self.0.insert_yaml("da.avail", secrets)
    }

    pub fn set_consensus_keys(&mut self, consensus_keys: RawConsensusKeys) -> anyhow::Result<()> {
        self.0
            .insert("consensus.validator_key", consensus_keys.validator_secret)?;
        self.0.insert("consensus.node_key", consensus_keys.node_secret)
    }

    pub fn set_consensus_node_key(&mut self, raw_key: &str) -> anyhow::Result<()> {
        self.0.insert("consensus.node_key", raw_key)
    }

    pub async fn save(self) -> anyhow::Result<()> {
        self.0.save().await
    }
}
