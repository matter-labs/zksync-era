use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, files::find_file, logger};
use zkstack_cli_types::{L1Network, ProverMode, VMOption, WalletCreation};
use zksync_basic_types::L2ChainId;

use crate::{
    consts::{
        CONFIGS_PATH, CONFIG_NAME, CONTRACTS_FILE, CONTRACTS_PATH, ECOSYSTEM_PATH, ERA_CHAIN_ID,
        ERC20_CONFIGS_FILE, ERC20_DEPLOYMENT_FILE, INITIAL_DEPLOYMENT_FILE,
        L1_CONTRACTS_FOUNDRY_INSIDE_CONTRACTS, LOCAL_ARTIFACTS_PATH, LOCAL_DB_PATH, WALLETS_FILE,
    },
    create_localhost_wallets,
    forge_interface::deploy_ecosystem::{
        input::{Erc20DeploymentConfig, InitialDeploymentConfig},
        output::{ERC20Tokens, Erc20Token},
    },
    source_files::SourceFiles,
    traits::{FileConfigTrait, FileConfigWithDefaultName, ReadConfig, SaveConfig},
    ChainConfig, ChainConfigInternal, CoreContractsConfig, WalletsConfig,
    PROVING_NETWORKS_DEPLOY_SCRIPT_PATH, PROVING_NETWORKS_PATH,
};

/// Ecosystem configuration file. This file is created in the chain
/// directory before network initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EcosystemConfigInternal {
    name: String,
    l1_network: L1Network,
    link_to_code: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    bellman_cuda_dir: Option<PathBuf>,
    chains: PathBuf,
    config: PathBuf,
    default_chain: String,
    era_chain_id: L2ChainId,
    prover_version: ProverMode,
    wallet_creation: WalletCreation,
    #[serde(skip_serializing_if = "Option::is_none")]
    era_source_files: Option<SourceFiles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zksync_os_source_files: Option<SourceFiles>,
}

/// Ecosystem configuration file. This file is created in the chain
/// directory before network initialization.
#[derive(Debug, Clone)]
pub struct EcosystemConfig {
    pub name: String,
    pub l1_network: L1Network,
    pub bellman_cuda_dir: Option<PathBuf>,
    pub chains: PathBuf,
    pub config: PathBuf,
    pub era_chain_id: L2ChainId,
    pub prover_version: ProverMode,
    pub wallet_creation: WalletCreation,
    default_chain: String,
    link_to_code: PathBuf,
    era_source_files: Option<SourceFiles>,
    zksync_os_source_files: Option<SourceFiles>,
    shell: OnceCell<Shell>,
}

impl EcosystemConfig {
    pub fn set_sources_path(
        &mut self,
        contracts_path: PathBuf,
        default_configs_path: PathBuf,
        vm_option: VMOption,
    ) {
        match vm_option {
            VMOption::EraVM => {
                self.zksync_os_source_files = Some(SourceFiles {
                    contracts_path,
                    default_configs_path,
                });
            }
            VMOption::ZKSyncOsVM => {
                self.era_source_files = Some(SourceFiles {
                    contracts_path,
                    default_configs_path,
                });
            }
        }
    }
}

impl Serialize for EcosystemConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.get_internal().serialize(serializer)
    }
}

impl ReadConfig for EcosystemConfig {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config: EcosystemConfigInternal = EcosystemConfigInternal::read(shell, path)?;

        let bellman_cuda_dir = config
            .bellman_cuda_dir
            .map(|dir| shell.current_dir().join(dir));
        Ok(EcosystemConfig {
            name: config.name.clone(),
            l1_network: config.l1_network,
            link_to_code: shell.current_dir().join(config.link_to_code),
            era_source_files: config.era_source_files.clone(),
            bellman_cuda_dir,
            chains: config.chains.clone(),
            config: config.config.clone(),
            default_chain: config.default_chain.clone(),
            era_chain_id: config.era_chain_id,
            prover_version: config.prover_version,
            wallet_creation: config.wallet_creation,
            shell: Default::default(),
            zksync_os_source_files: config.zksync_os_source_files.clone(),
        })
    }
}

impl FileConfigWithDefaultName for EcosystemConfig {
    const FILE_NAME: &'static str = CONFIG_NAME;
}

impl FileConfigTrait for EcosystemConfigInternal {}

impl FileConfigTrait for EcosystemConfig {}

impl EcosystemConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        l1_network: L1Network,
        link_to_code: PathBuf,
        bellman_cuda_dir: Option<PathBuf>,
        chains: PathBuf,
        config: PathBuf,
        default_chain: String,
        era_chain_id: L2ChainId,
        prover_version: ProverMode,
        wallet_creation: WalletCreation,
        shell: OnceCell<Shell>,
    ) -> Self {
        Self {
            name,
            l1_network,
            link_to_code,
            bellman_cuda_dir,
            chains,
            config,
            default_chain,
            era_chain_id,
            prover_version,
            wallet_creation,
            shell,
            era_source_files: None,
            zksync_os_source_files: None,
        }
    }

    fn get_shell(&self) -> &Shell {
        self.shell.get().expect("Must be initialized")
    }

    pub(crate) fn from_file(shell: &Shell) -> Result<Self, EcosystemConfigFromFileError> {
        let Ok(path) = find_file(shell, &shell.current_dir(), CONFIG_NAME) else {
            return Err(EcosystemConfigFromFileError::NotExists {
                path: shell.current_dir(),
            });
        };

        shell.change_dir(&path);

        let ecosystem = match EcosystemConfig::read(shell, CONFIG_NAME) {
            Ok(mut config) => {
                config.shell = shell.clone().into();
                config
            }
            Err(_) => {
                // Try to deserialize with chain config, if it's successful, likely we are in the folder
                // with chain and we will find the ecosystem config somewhere in parent directories
                let chain_config = ChainConfigInternal::read(shell, CONFIG_NAME)
                    .map_err(|err| EcosystemConfigFromFileError::InvalidConfig { source: err })?;
                logger::info(format!("You are in a directory with chain config, default chain for execution has changed to {}", &chain_config.name));

                let current_dir = shell.current_dir();
                let Some(parent) = current_dir.parent() else {
                    return Err(EcosystemConfigFromFileError::NotExists { path });
                };
                // Try to find ecosystem somewhere in parent directories
                shell.change_dir(parent);
                let mut ecosystem_config = EcosystemConfig::from_file(shell)?;
                // change the default chain for using it in later executions
                ecosystem_config.default_chain = chain_config.name;
                ecosystem_config
            }
        };
        Ok(ecosystem)
    }

    pub fn set_default_chain(&mut self, name: String) {
        self.default_chain = name
    }

    pub fn current_chain(&self) -> &str {
        global_config()
            .chain_name
            .as_deref()
            .unwrap_or(self.default_chain.as_ref())
    }

    pub fn load_chain(&self, name: Option<String>) -> anyhow::Result<ChainConfig> {
        let name = name.unwrap_or(self.default_chain.clone());
        self.load_chain_inner(&name)
    }

    pub fn load_current_chain(&self) -> anyhow::Result<ChainConfig> {
        self.load_chain_inner(self.current_chain())
    }

    fn load_chain_inner(&self, name: &str) -> anyhow::Result<ChainConfig> {
        let path = self.chains.join(name);

        let config = ChainConfigInternal::read(self.get_shell(), path.join(CONFIG_NAME).clone())?;

        Ok(ChainConfig::new(
            config.id,
            config.name,
            config.chain_id,
            config.prover_version,
            self.l1_network,
            path,
            config.link_to_code.unwrap_or(self.link_to_code.clone()),
            config.rocks_db_path,
            // It's required for backward compatibility
            config
                .artifacts_path
                .unwrap_or_else(|| self.get_chain_artifacts_path(name)),
            config.configs,
            config.external_node_config_path,
            config.l1_batch_commit_data_generator_mode,
            config.base_token,
            config.wallet_creation,
            self.get_shell().clone().into(),
            config.legacy_bridge,
            config.evm_emulator,
            config.tight_ports,
            config.vm_option,
            config.contracts_source_path,
        ))
    }

    pub fn get_initial_deployment_config(&self) -> anyhow::Result<InitialDeploymentConfig> {
        InitialDeploymentConfig::read(self.get_shell(), self.config.join(INITIAL_DEPLOYMENT_FILE))
    }

    pub fn get_erc20_deployment_config(&self) -> anyhow::Result<Erc20DeploymentConfig> {
        Erc20DeploymentConfig::read(self.get_shell(), self.config.join(ERC20_DEPLOYMENT_FILE))
    }
    pub fn get_erc20_tokens(&self) -> Vec<Erc20Token> {
        ERC20Tokens::read(self.get_shell(), self.config.join(ERC20_CONFIGS_FILE))
            .map(|tokens| tokens.tokens.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_wallets(&self) -> anyhow::Result<WalletsConfig> {
        let path = self.config.join(WALLETS_FILE);
        if self.get_shell().path_exists(&path) {
            return WalletsConfig::read(self.get_shell(), &path);
        }
        if self.wallet_creation == WalletCreation::Localhost {
            // Use 0 id for ecosystem  wallets
            let wallets = create_localhost_wallets(self.get_shell(), &self.link_to_code, 0)?;
            wallets.save(self.get_shell(), &path)?;
            return Ok(wallets);
        }
        anyhow::bail!("Wallets configs has not been found");
    }

    pub fn get_contracts_config(&self) -> anyhow::Result<CoreContractsConfig> {
        CoreContractsConfig::read_with_fallback(
            self.get_shell(),
            self.config.join(CONTRACTS_FILE),
            VMOption::EraVM,
        )
    }

    pub fn path_to_proving_networks(&self) -> PathBuf {
        self.link_to_code.join(PROVING_NETWORKS_PATH)
    }

    pub fn path_to_proving_networks_deploy_script(&self) -> PathBuf {
        self.path_to_proving_networks()
            .join(PROVING_NETWORKS_DEPLOY_SCRIPT_PATH)
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

    /// Path to the predefined ecosystem configs
    pub fn get_preexisting_configs_path(&self) -> PathBuf {
        self.link_to_code.join(ECOSYSTEM_PATH)
    }

    pub fn get_chain_rocks_db_path(&self, chain_name: &str) -> PathBuf {
        self.chains.join(chain_name).join(LOCAL_DB_PATH)
    }

    pub fn get_chain_artifacts_path(&self, chain_name: &str) -> PathBuf {
        self.chains.join(chain_name).join(LOCAL_ARTIFACTS_PATH)
    }

    fn get_internal(&self) -> EcosystemConfigInternal {
        let bellman_cuda_dir = self
            .bellman_cuda_dir
            .clone()
            .map(|dir| self.get_shell().current_dir().join(dir));
        EcosystemConfigInternal {
            name: self.name.clone(),
            l1_network: self.l1_network,
            link_to_code: self.get_shell().current_dir().join(&self.link_to_code),
            bellman_cuda_dir,
            chains: self.chains.clone(),
            config: self.config.clone(),
            default_chain: self.default_chain.clone(),
            era_chain_id: self.era_chain_id,
            prover_version: self.prover_version,
            wallet_creation: self.wallet_creation,
            era_source_files: self.era_source_files.clone(),
            zksync_os_source_files: self.zksync_os_source_files.clone(),
        }
    }

    pub fn get_source_files(&self, vm_option: VMOption) -> Option<&SourceFiles> {
        match vm_option {
            VMOption::EraVM => self.era_source_files.as_ref(),
            VMOption::ZKSyncOsVM => self.zksync_os_source_files.as_ref(),
        }
    }

    pub fn default_configs_path_for_ctm(&self, vm_option: VMOption) -> PathBuf {
        self.get_source_files(vm_option)
            .map(|files| files.default_configs_path.clone())
            .unwrap_or_else(|| {
                if vm_option.is_zksync_os() {
                    logger::warn("Warning: zksync_os_contracts_path is not set, falling back to default contracts path.");
                }
                self.link_to_code.join(CONFIGS_PATH)
            })
    }

    pub fn contracts_path_for_ctm(&self, vm_option: VMOption) -> PathBuf {
        self.get_source_files(vm_option)
            .map(|files| files.contracts_path.clone())
            .unwrap_or_else(|| {
                if vm_option.is_zksync_os(){
                    logger::warn("Warning: zksync_os_contracts_path is not set, falling back to default contracts path.");
                }
                self.link_to_code.join(CONTRACTS_PATH)
            })
    }

    pub fn path_to_foundry_scripts_for_ctm(&self, vm_option: VMOption) -> PathBuf {
        self.contracts_path_for_ctm(vm_option)
            .join(L1_CONTRACTS_FOUNDRY_INSIDE_CONTRACTS)
    }

    pub fn link_to_code(&self) -> PathBuf {
        self.link_to_code.clone()
    }

    pub fn zksync_os_exist(&self) -> bool {
        self.zksync_os_source_files.is_some()
    }
}

/// Result of checking if the ecosystem exists.
#[derive(Error, Debug)]
pub enum EcosystemConfigFromFileError {
    #[error("Ecosystem configuration not found (Could not find 'ZkStack.toml' in {path:?}: Make sure you have created an ecosystem & are in the new folder `cd path/to/ecosystem/name`)"
    )]
    NotExists { path: PathBuf },
    #[error("Invalid ecosystem configuration")]
    InvalidConfig { source: anyhow::Error },
}

pub fn get_default_era_chain_id() -> L2ChainId {
    L2ChainId::from(ERA_CHAIN_ID)
}

pub fn get_link_to_prover(link_to_code: &Path) -> PathBuf {
    link_to_code.join("prover")
}
