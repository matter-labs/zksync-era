use crate::traits::{ReadConfig, SaveConfig};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
    pub db: RocksDBConfig,
    pub eth: EthConfig,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl ReadConfig for GeneralConfig {}
impl SaveConfig for GeneralConfig {}

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
