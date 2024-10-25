use anyhow::Context;
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

            let l1_network = chain_internal.l1_network.context("L1 Network not found")?;
            let link_to_code = chain_internal
                .link_to_code
                .context("Link to code not found")?;
            let artifacts = chain_internal
                .artifacts_path
                .context("Artifacts path not found")?;

            let chain = ChainConfig {
                id: chain_internal.id,
                name: chain_internal.name,
                chain_id: chain_internal.chain_id,
                prover_version: chain_internal.prover_version,
                configs: chain_internal.configs,
                rocks_db_path: chain_internal.rocks_db_path,
                external_node_config_path: chain_internal.external_node_config_path,
                l1_network,
                l1_batch_commit_data_generator_mode: chain_internal
                    .l1_batch_commit_data_generator_mode,
                base_token: chain_internal.base_token,
                wallet_creation: chain_internal.wallet_creation,
                legacy_bridge: chain_internal.legacy_bridge,
                link_to_code,
                artifacts,
                shell: shell.clone().into(),
                evm_emulator: chain_internal.evm_emulator,
            };

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
