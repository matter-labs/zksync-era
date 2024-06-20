use std::{cell::OnceCell, path::PathBuf};

use path_absolutize::Absolutize;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use types::{ChainId, L1Network, ProverMode, WalletCreation};
use xshell::Shell;

use crate::{
    consts::{
        CONFIGS_PATH, CONFIG_NAME, CONTRACTS_FILE, ECOSYSTEM_PATH, ERA_CHAIN_ID,
        ERC20_DEPLOYMENT_FILE, INITIAL_DEPLOYMENT_FILE, L1_CONTRACTS_FOUNDRY, LOCAL_DB_PATH,
        WALLETS_FILE,
    },
    create_localhost_wallets,
    forge_interface::deploy_ecosystem::input::{Erc20DeploymentConfig, InitialDeploymentConfig},
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig},
    ChainConfig, ChainConfigInternal, ContractsConfig, WalletsConfig,
};

/// Ecosystem configuration file. This file is created in the chain
/// directory before network initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EcosystemConfigInternal {
    pub name: String,
    pub l1_network: L1Network,
    pub link_to_code: PathBuf,
    pub chains: PathBuf,
    pub config: PathBuf,
    pub default_chain: String,
    pub era_chain_id: ChainId,
    pub prover_version: ProverMode,
    pub wallet_creation: WalletCreation,
}

/// Ecosystem configuration file. This file is created in the chain
/// directory before network initialization.
#[derive(Debug, Clone)]
pub struct EcosystemConfig {
    pub name: String,
    pub l1_network: L1Network,
    pub link_to_code: PathBuf,
    pub chains: PathBuf,
    pub config: PathBuf,
    pub default_chain: String,
    pub era_chain_id: ChainId,
    pub prover_version: ProverMode,
    pub wallet_creation: WalletCreation,
    pub shell: OnceCell<Shell>,
}

impl Serialize for EcosystemConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.get_internal().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EcosystemConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let config: EcosystemConfigInternal = Deserialize::deserialize(deserializer)?;
        Ok(EcosystemConfig {
            name: config.name.clone(),
            l1_network: config.l1_network,
            link_to_code: config
                .link_to_code
                .absolutize()
                .expect("Failed to parse zksync-era path")
                .to_path_buf(),
            chains: config.chains.clone(),
            config: config.config.clone(),
            default_chain: config.default_chain.clone(),
            era_chain_id: config.era_chain_id,
            prover_version: config.prover_version,
            wallet_creation: config.wallet_creation,
            shell: Default::default(),
        })
    }
}

impl FileConfigWithDefaultName for EcosystemConfig {
    const FILE_NAME: &'static str = CONFIG_NAME;
}

impl EcosystemConfig {
    fn get_shell(&self) -> &Shell {
        self.shell.get().expect("Must be initialized")
    }

    pub fn from_file(shell: &Shell) -> Result<Self, EcosystemConfigFromFileError> {
        let path = PathBuf::from(CONFIG_NAME);
        if !shell.path_exists(path) {
            return Err(EcosystemConfigFromFileError::NotExists);
        }

        let mut config = EcosystemConfig::read(shell, CONFIG_NAME)
            .map_err(|e| EcosystemConfigFromFileError::InvalidConfig { source: e })?;
        config.shell = shell.clone().into();

        Ok(config)
    }

    pub fn load_chain(&self, name: Option<String>) -> Option<ChainConfig> {
        let name = name.unwrap_or(self.default_chain.clone());
        self.load_chain_inner(&name)
    }

    fn load_chain_inner(&self, name: &str) -> Option<ChainConfig> {
        let path = self.chains.join(name).join(CONFIG_NAME);
        let config = ChainConfigInternal::read(self.get_shell(), path).ok()?;

        Some(ChainConfig {
            id: config.id,
            name: config.name,
            chain_id: config.chain_id,
            prover_version: config.prover_version,
            configs: config.configs,
            l1_batch_commit_data_generator_mode: config.l1_batch_commit_data_generator_mode,
            l1_network: self.l1_network,
            link_to_code: self
                .link_to_code
                .absolutize()
                .expect("Failed to parse zksync-era path")
                .into(),
            base_token: config.base_token,
            rocks_db_path: config.rocks_db_path,
            wallet_creation: config.wallet_creation,
            shell: self.get_shell().clone().into(),
        })
    }

    pub fn get_initial_deployment_config(&self) -> anyhow::Result<InitialDeploymentConfig> {
        InitialDeploymentConfig::read(self.get_shell(), self.config.join(INITIAL_DEPLOYMENT_FILE))
    }

    pub fn get_erc20_deployment_config(&self) -> anyhow::Result<Erc20DeploymentConfig> {
        Erc20DeploymentConfig::read(self.get_shell(), self.config.join(ERC20_DEPLOYMENT_FILE))
    }

    pub fn get_wallets(&self) -> anyhow::Result<WalletsConfig> {
        let path = self.config.join(WALLETS_FILE);
        if let Ok(wallets) = WalletsConfig::read(self.get_shell(), &path) {
            return Ok(wallets);
        }
        if self.wallet_creation == WalletCreation::Localhost {
            // Use 0 id for ecosystem  wallets
            let wallets = create_localhost_wallets(self.get_shell(), &self.link_to_code, 0)?;
            wallets.save(self.get_shell(), &path)?;
            return Ok(wallets);
        }
        anyhow::bail!("Wallets configs has not been found");
    }

    pub fn get_contracts_config(&self) -> anyhow::Result<ContractsConfig> {
        ContractsConfig::read(self.get_shell(), self.config.join(CONTRACTS_FILE))
    }

    pub fn path_to_foundry(&self) -> PathBuf {
        self.link_to_code.join(L1_CONTRACTS_FOUNDRY)
    }

    pub fn list_of_chains(&self) -> Vec<String> {
        self.get_shell()
            .read_dir(&self.chains)
            .unwrap()
            .iter()
            .filter_map(|file| {
                if file.is_dir() {
                    file.file_name().map(|a| a.to_str().unwrap().to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_default_configs_path(&self) -> PathBuf {
        self.link_to_code.join(CONFIGS_PATH)
    }

    /// Path to the predefined ecosystem configs
    pub fn get_preexisting_configs_path(&self) -> PathBuf {
        self.link_to_code.join(ECOSYSTEM_PATH)
    }

    pub fn get_chain_rocks_db_path(&self, chain_name: &str) -> PathBuf {
        self.chains.join(chain_name).join(LOCAL_DB_PATH)
    }

    fn get_internal(&self) -> EcosystemConfigInternal {
        EcosystemConfigInternal {
            name: self.name.clone(),
            l1_network: self.l1_network,
            link_to_code: self
                .link_to_code
                .absolutize()
                .expect("Failed to parse zksync-era path")
                .into(),
            chains: self.chains.clone(),
            config: self.config.clone(),
            default_chain: self.default_chain.clone(),
            era_chain_id: self.era_chain_id,
            prover_version: self.prover_version,
            wallet_creation: self.wallet_creation,
        }
    }
}

/// Result of checking if the ecosystem exists.
#[derive(Error, Debug)]
pub enum EcosystemConfigFromFileError {
    #[error("Ecosystem configuration not found (make sure you have created one `zk_inception ecosystem create` + are in the ecosystem folder `cd path/to/ecosystem/name`)")]
    NotExists,
    #[error("Invalid ecosystem configuration")]
    InvalidConfig { source: anyhow::Error },
}

pub fn get_default_era_chain_id() -> ChainId {
    ERA_CHAIN_ID
}
