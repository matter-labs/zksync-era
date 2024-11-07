use xshell::Shell;

use crate::{ChainConfig, ChainConfigInternal, EcosystemConfig};

pub enum ZkStackConfig {
    EcosystemConfig(EcosystemConfig),
    ChainConfig(ChainConfig),
}

impl ZkStackConfig {
    fn from_file(shell: &Shell) -> anyhow::Result<ZkStackConfig> {
        if let Ok(ecosystem) = EcosystemConfig::from_file(shell) {
            Ok(ZkStackConfig::EcosystemConfig(ecosystem))
        } else {
            let chain_internal = ChainConfigInternal::from_file(shell)?;
            let chain = ChainConfig::from_internal(chain_internal, shell.clone())?;
            Ok(ZkStackConfig::ChainConfig(chain))
        }
    }

    pub fn load_current_chain(shell: &Shell) -> anyhow::Result<ChainConfig> {
        match ZkStackConfig::from_file(shell)? {
            ZkStackConfig::EcosystemConfig(ecosystem) => ecosystem.load_current_chain(),
            ZkStackConfig::ChainConfig(chain) => Ok(chain),
        }
    }
}
