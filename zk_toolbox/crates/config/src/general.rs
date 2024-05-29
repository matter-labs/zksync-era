use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{consts::GENERAL_FILE, traits::FileConfigWithDefaultName};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
    pub db: RocksDBConfig,
    pub eth: EthConfig,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl FileConfigWithDefaultName for GeneralConfig {
    const FILE_NAME: &'static str = GENERAL_FILE;
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EthConfig {
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
