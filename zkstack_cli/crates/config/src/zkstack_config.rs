use std::path::PathBuf;

use anyhow::bail;
use xshell::Shell;

use crate::{ChainConfig, ChainConfigInternal, EcosystemConfig, EcosystemConfigFromFileError};

pub enum ZkStackConfig {
    EcosystemConfig(EcosystemConfig),
    ChainConfig(ChainConfig),
}

impl ZkStackConfig {
    pub fn from_file(shell: &Shell) -> anyhow::Result<ZkStackConfig> {
        let current_dir = shell.current_dir();
        if let Ok(ecosystem) = EcosystemConfig::from_file(shell) {
            return Ok(ZkStackConfig::EcosystemConfig(ecosystem));
        }

        shell.change_dir(&current_dir);
        if let Ok(chain_internal) = ChainConfigInternal::from_file(shell) {
            if let Ok(chain) = ChainConfig::from_internal(chain_internal, shell.clone()) {
                return Ok(ZkStackConfig::ChainConfig(chain));
            }
        }

        bail!("Could not find `ZkStack.yaml` for ecosystem or chain in `{current_dir:?}` or any parent directory.");
    }

    pub fn current_chain(shell: &Shell) -> anyhow::Result<ChainConfig> {
        match ZkStackConfig::from_file(shell)? {
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem.load_current_chain(),
            ZkStackConfig::ChainConfig(chain) => Ok(chain),
        }
    }

    pub fn ecosystem(shell: &Shell) -> Result<EcosystemConfig, EcosystemConfigFromFileError> {
        EcosystemConfig::from_file(shell)
    }
}

impl ZkStackConfigTrait for ZkStackConfig {
    fn link_to_code(&self) -> PathBuf {
        match self {
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem.link_to_code(),
            ZkStackConfig::ChainConfig(chain) => chain.link_to_code().clone(),
        }
    }

    fn default_configs_path(&self) -> PathBuf {
        match self {
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem
                // TODO pass zksync_os from config
                .default_configs_path_for_ctm(false)
                .clone(),
            ZkStackConfig::ChainConfig(chain) => chain.default_configs_path().clone(),
        }
    }

    fn contracts_path(&self) -> PathBuf {
        match self {
            ZkStackConfig::EcosystemConfig(ecosystem) => {
                // For default zksync config files we use default paths to foundry scripts
                ecosystem.contracts_path_for_ctm(false)
            }
            ZkStackConfig::ChainConfig(chain) => chain.contracts_path(),
        }
    }

    fn path_to_foundry_scripts(&self) -> PathBuf {
        match self {
            ZkStackConfig::EcosystemConfig(ecosystem) => {
                // For default zksync config files we use default paths to foundry scripts
                ecosystem.path_to_foundry_scripts_for_ctm(false)
            }
            ZkStackConfig::ChainConfig(chain) => chain.path_to_foundry_scripts().clone(),
        }
    }
}

pub trait ZkStackConfigTrait {
    /// Link to the repository, please use it with a caution and prefer specific links
    fn link_to_code(&self) -> PathBuf;

    /// Path to the directory with default configs inside the repository.
    /// It's the configs, that used as templates for new chains, in era they are placed `etc/env/file_based`
    fn default_configs_path(&self) -> PathBuf;
    /// Path to the directory with contracts, that represents era-contracts repo
    fn contracts_path(&self) -> PathBuf;

    /// Path to the directory with L1 Foundry contracts
    fn path_to_foundry_scripts(&self) -> PathBuf;
}
