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
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem.link_to_code().clone(),
            ZkStackConfig::ChainConfig(chain) => chain.link_to_code().clone(),
        }
    }

    fn default_configs_path(&self) -> PathBuf {
        match self {
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem.default_configs_path().clone(),
            ZkStackConfig::ChainConfig(chain) => chain.default_configs_path().clone(),
        }
    }

    fn contracts_path(&self) -> PathBuf {
        match self {
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem.contracts_path().clone(),
            ZkStackConfig::ChainConfig(chain) => chain.contracts_path().clone(),
        }
    }

    fn path_to_l1_foundry(&self) -> PathBuf {
        match self {
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem.path_to_l1_foundry().clone(),
            ZkStackConfig::ChainConfig(chain) => chain.path_to_l1_foundry().clone(),
        }
    }
}

pub trait ZkStackConfigTrait {
    /// Link to the repository, please use it with a caution and prefer specific links
    fn link_to_code(&self) -> PathBuf;

    fn default_configs_path(&self) -> PathBuf;
    fn contracts_path(&self) -> PathBuf;

    fn path_to_l1_foundry(&self) -> PathBuf;
}
