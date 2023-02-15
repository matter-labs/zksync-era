#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use serde::Deserialize;

pub use crate::configs::{
    ApiConfig, ChainConfig, ContractVerifierConfig, ContractsConfig, DBConfig, ETHClientConfig,
    ETHSenderConfig, ETHWatchConfig, FetcherConfig, GasAdjusterConfig, ObjectStoreConfig,
    ProverConfig, ProverConfigs,
};

pub mod configs;
pub mod constants;
pub mod test_config;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ZkSyncConfig {
    pub api: ApiConfig,
    pub chain: ChainConfig,
    pub contracts: ContractsConfig,
    pub db: DBConfig,
    pub eth_client: ETHClientConfig,
    pub eth_sender: ETHSenderConfig,
    pub eth_watch: ETHWatchConfig,
    pub fetcher: FetcherConfig,
    pub prover: ProverConfigs,
    pub object_store: ObjectStoreConfig,
}

impl ZkSyncConfig {
    pub fn from_env() -> Self {
        Self {
            api: ApiConfig::from_env(),
            chain: ChainConfig::from_env(),
            contracts: ContractsConfig::from_env(),
            db: DBConfig::from_env(),
            eth_client: ETHClientConfig::from_env(),
            eth_sender: ETHSenderConfig::from_env(),
            eth_watch: ETHWatchConfig::from_env(),
            fetcher: FetcherConfig::from_env(),
            prover: ProverConfigs::from_env(),
            object_store: ObjectStoreConfig::from_env(),
        }
    }

    pub fn default_db() -> DBConfig {
        DBConfig::default()
    }
}
