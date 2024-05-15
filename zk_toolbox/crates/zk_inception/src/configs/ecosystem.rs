use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    configs::{
        forge_interface::deploy_ecosystem::input::{
            Erc20DeploymentConfig, InitialDeploymentConfig,
        },
        ContractsConfig, HyperchainConfig, HyperchainConfigInternal, ReadConfig, SaveConfig,
        WalletsConfig,
    },
    consts::{
        CONFIG_NAME, CONTRACTS_FILE, ERC20_DEPLOYMENT_FILE, INITIAL_DEPLOYMENT_FILE,
        L1_CONTRACTS_FOUNDRY, LOCAL_CONFIGS_PATH, WALLETS_FILE,
    },
    types::{ChainId, L1Network, ProverMode},
};

/// Ecosystem configuration file. This file is created in the hyperchain
/// directory before network initialization.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EcosystemConfig {
    pub name: String,
    pub l1_network: L1Network,
    pub link_to_code: PathBuf,
    pub hyperchains: PathBuf,
    pub config: PathBuf,
    pub default_hyperchain: String,
    pub l1_rpc_url: String,
    pub era_chain_id: ChainId,
    pub prover_version: ProverMode,
}

impl ReadConfig for EcosystemConfig {}
impl SaveConfig for EcosystemConfig {}

impl EcosystemConfig {
    pub fn from_file() -> Result<Self, EcosystemConfigFromFileError> {
        let path = PathBuf::from(CONFIG_NAME);
        if !path.exists() {
            return Err(EcosystemConfigFromFileError::NotExists);
        }

        let config = EcosystemConfig::read(CONFIG_NAME)
            .map_err(|_| EcosystemConfigFromFileError::InvalidConfig)?;
        Ok(config)
    }

    pub fn load_hyperchain(&self, name: Option<String>) -> Option<HyperchainConfig> {
        let name = name.unwrap_or(self.default_hyperchain.clone());
        self.load_hyperchain_inner(&name)
    }

    fn load_hyperchain_inner(&self, name: &str) -> Option<HyperchainConfig> {
        let path = self
            .hyperchains
            .join(name)
            .join(LOCAL_CONFIGS_PATH)
            .join(CONFIG_NAME);
        let config = HyperchainConfigInternal::read(path).ok()?;

        Some(HyperchainConfig {
            id: config.id,
            name: config.name,
            chain_id: config.chain_id,
            prover_version: config.prover_version,
            configs: config.configs,
            l1_batch_commit_data_generator_mode: config.l1_batch_commit_data_generator_mode,
            l1_network: self.l1_network,
            link_to_code: self.link_to_code.clone(),
            base_token: config.base_token,
            rocks_db_path: config.rocks_db_path,
        })
    }

    pub fn get_initial_deployment_config(&self) -> anyhow::Result<InitialDeploymentConfig> {
        InitialDeploymentConfig::read(self.config.join(INITIAL_DEPLOYMENT_FILE))
    }

    pub fn get_erc20_deployment_config(&self) -> anyhow::Result<Erc20DeploymentConfig> {
        Erc20DeploymentConfig::read(self.config.join(ERC20_DEPLOYMENT_FILE))
    }

    pub fn get_wallets(&self) -> anyhow::Result<WalletsConfig> {
        WalletsConfig::read(self.config.join(WALLETS_FILE))
    }
    pub fn get_contracts_config(&self) -> anyhow::Result<ContractsConfig> {
        ContractsConfig::read(self.config.join(CONTRACTS_FILE))
    }

    pub fn path_to_foundry(&self) -> PathBuf {
        self.link_to_code.join(L1_CONTRACTS_FOUNDRY)
    }

    pub fn list_of_hyperchains(&self) -> Vec<String> {
        std::fs::read_dir(&self.hyperchains)
            .unwrap()
            .filter_map(|file| {
                let file = file.unwrap();
                let Ok(file_type) = file.file_type() else {
                    return None;
                };
                if file_type.is_dir() {
                    file.file_name().into_string().ok()
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Result of checking if the ecosystem exists.
#[derive(Error, Debug)]

pub enum EcosystemConfigFromFileError {
    #[error("Invalid ecosystem configuration")]
    InvalidConfig,
    #[error("Ecosystem configuration not found")]
    NotExists,
}
