use xshell::Shell;

use crate::{ChainConfigInternal, EcosystemConfig};

pub enum ZkStackConfig {
    EcosystemConfig(EcosystemConfig),
    ChainConfig(ChainConfigInternal),
}

impl ZkStackConfig {
    pub fn from_file(shell: &Shell) -> anyhow::Result<ZkStackConfig> {
        if let Ok(ecosystem) = EcosystemConfig::from_file(shell) {
            Ok(ZkStackConfig::EcosystemConfig(ecosystem))
        } else {
            let chain = ChainConfigInternal::from_file(shell)?;
            Ok(ZkStackConfig::ChainConfig(chain))
        }
    }
}
