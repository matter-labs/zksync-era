use std::{cell::OnceCell, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use xshell::Shell;

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
        L1_CONTRACTS_FOUNDRY, WALLETS_FILE,
    },
    types::{ChainId, L1Network, ProverMode},
    wallets::{create_localhost_wallets, WalletCreation},
};

/// Ecosystem configuration file. This file is created in the hyperchain
/// directory before network initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EcosystemConfigInternal {
    pub name: String,
    pub l1_network: L1Network,
    pub link_to_code: PathBuf,
    pub hyperchains: PathBuf,
    pub config: PathBuf,
    pub default_hyperchain: String,
    pub l1_rpc_url: String,
    pub era_chain_id: ChainId,
    pub prover_version: ProverMode,
    pub wallet_creation: WalletCreation,
}

/// Ecosystem configuration file. This file is created in the hyperchain
/// directory before network initialization.
#[derive(Debug, Clone)]
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
            link_to_code: config.link_to_code.clone(),
            hyperchains: config.hyperchains.clone(),
            config: config.config.clone(),
            default_hyperchain: config.default_hyperchain.clone(),
            l1_rpc_url: config.l1_rpc_url.clone(),
            era_chain_id: config.era_chain_id,
            prover_version: config.prover_version,
            wallet_creation: config.wallet_creation,
            shell: Default::default(),
        })
    }
}

impl ReadConfig for EcosystemConfig {}
impl SaveConfig for EcosystemConfig {}

impl EcosystemConfig {
    fn get_shell(&self) -> &Shell {
        self.shell.get().expect("Must be initialized")
    }

    pub fn from_file(shell: &Shell) -> Result<Self, EcosystemConfigFromFileError> {
        let path = PathBuf::from(format!("./{}", CONFIG_NAME));
        if !path.exists() {
            return Err(EcosystemConfigFromFileError::NotExists);
        }

        let mut config = EcosystemConfig::read(shell, CONFIG_NAME)
            .map_err(|_| EcosystemConfigFromFileError::InvalidConfig)?;
        config.shell = shell.clone().into();

        Ok(config)
    }

    pub fn load_hyperchain(&self, name: Option<String>) -> Option<HyperchainConfig> {
        let name = name.unwrap_or(self.default_hyperchain.clone());
        self.load_hyperchain_inner(&name)
    }

    fn load_hyperchain_inner(&self, name: &str) -> Option<HyperchainConfig> {
        let path = self.hyperchains.join(name).join(CONFIG_NAME);
        let config = HyperchainConfigInternal::read(self.get_shell(), path).ok()?;

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

    pub fn list_of_hyperchains(&self) -> Vec<String> {
        self.get_shell()
            .read_dir(&self.hyperchains)
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

    fn get_internal(&self) -> EcosystemConfigInternal {
        EcosystemConfigInternal {
            name: self.name.clone(),
            l1_network: self.l1_network,
            link_to_code: self.link_to_code.clone(),
            hyperchains: self.hyperchains.clone(),
            config: self.config.clone(),
            default_hyperchain: self.default_hyperchain.clone(),
            l1_rpc_url: self.l1_rpc_url.clone(),
            era_chain_id: self.era_chain_id,
            prover_version: self.prover_version,
            wallet_creation: self.wallet_creation,
        }
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
