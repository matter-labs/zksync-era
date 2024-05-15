use std::path::PathBuf;

use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    configs::{ReadConfig, SaveConfig},
    types::{ChainId, L1BatchCommitDataGeneratorMode},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenesisConfig {
    pub l2_chain_id: ChainId,
    pub l1_chain_id: u32,
    pub l1_batch_commit_data_generator_mode: Option<L1BatchCommitDataGeneratorMode>,
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub fee_account: Address,
    pub genesis_batch_commitment: H256,
    pub genesis_rollup_leaf_index: u32,
    pub genesis_root: H256,
    pub genesis_protocol_version: u64,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl ReadConfig for GenesisConfig {}
impl SaveConfig for GenesisConfig {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EthConfig {
    pub web3_url: String,
    pub sender: EthSender,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EthSender {
    pub proof_sending_mode: String,
    pub pubdata_sending_mode: String,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PostgresConfig {
    pub server_url: String,
    pub prover_url: String,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
    pub postgres: PostgresConfig,
    pub db: RocksDBConfig,
    pub eth: EthConfig,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RocksDBConfig {
    pub state_keeper_db_path: PathBuf,
    pub merkle_tree: MerkleTreeDB,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MerkleTreeDB {
    pub path: PathBuf,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl ReadConfig for GeneralConfig {}
impl SaveConfig for GeneralConfig {}

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
