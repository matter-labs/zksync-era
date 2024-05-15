use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xshell::Shell;

use crate::{
    configs::{ContractsConfig, GenesisConfig, ReadConfig, SaveConfig, WalletsConfig},
    consts::{CONTRACTS_FILE, GENESIS_FILE, L1_CONTRACTS_FOUNDRY, WALLETS_FILE},
    types::{BaseToken, ChainId, L1BatchCommitDataGeneratorMode, L1Network, ProverMode},
};

/// Hyperchain configuration file. This file is created in the hyperchain
/// directory before network initialization.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HyperchainConfigInternal {
    // The id of hyperchain on this machine allows to easily setup multiple hyperchains,
    // needs for local setups only
    pub id: u32,
    pub name: String,
    pub chain_id: ChainId,
    pub prover_version: ProverMode,
    pub configs: PathBuf,
    pub rocks_db_path: PathBuf,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    pub base_token: BaseToken,
}

/// Hyperchain configuration file. This file is created in the hyperchain
/// directory before network initialization.
#[derive(Debug, Serialize, Deserialize)]
pub struct HyperchainConfig {
    pub id: u32,
    pub name: String,
    pub chain_id: ChainId,
    pub prover_version: ProverMode,
    pub l1_network: L1Network,
    pub link_to_code: PathBuf,
    pub rocks_db_path: PathBuf,
    pub configs: PathBuf,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    pub base_token: BaseToken,
}

impl HyperchainConfig {
    pub fn get_genesis_config(&self) -> anyhow::Result<GenesisConfig> {
        GenesisConfig::read(self.configs.join(GENESIS_FILE))
    }

    pub fn get_wallets_config(&self) -> anyhow::Result<WalletsConfig> {
        WalletsConfig::read(self.configs.join(WALLETS_FILE))
    }
    pub fn get_contracts_config(&self) -> anyhow::Result<ContractsConfig> {
        ContractsConfig::read(self.configs.join(CONTRACTS_FILE))
    }

    pub fn path_to_foundry(&self) -> PathBuf {
        self.link_to_code.join(L1_CONTRACTS_FOUNDRY)
    }
}

impl SaveConfig for HyperchainConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let config = HyperchainConfigInternal {
            id: self.id,
            name: self.name.clone(),
            chain_id: self.chain_id,
            prover_version: self.prover_version,
            configs: self.configs.clone(),
            rocks_db_path: self.rocks_db_path.clone(),
            l1_batch_commit_data_generator_mode: self.l1_batch_commit_data_generator_mode,
            base_token: self.base_token.clone(),
        };
        config.save(shell, path)
    }
}

impl ReadConfig for HyperchainConfigInternal {}
impl SaveConfig for HyperchainConfigInternal {}
