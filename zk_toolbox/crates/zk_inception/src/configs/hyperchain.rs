use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize, Serializer};
use xshell::Shell;

use crate::{
    configs::{ContractsConfig, GenesisConfig, ReadConfig, SaveConfig, WalletsConfig},
    consts::{CONTRACTS_FILE, GENESIS_FILE, L1_CONTRACTS_FOUNDRY, WALLETS_FILE},
    types::{BaseToken, ChainId, L1BatchCommitDataGeneratorMode, L1Network, ProverMode},
    wallets::{create_localhost_wallets, WalletCreation},
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
    pub wallet_creation: WalletCreation,
}

/// Hyperchain configuration file. This file is created in the hyperchain
/// directory before network initialization.
#[derive(Debug)]
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
    pub wallet_creation: WalletCreation,
    pub shell: OnceCell<Shell>,
}

impl Serialize for HyperchainConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.get_internal().serialize(serializer)
    }
}

impl HyperchainConfig {
    pub(crate) fn get_shell(&self) -> &Shell {
        self.shell.get().expect("Not initialized")
    }

    pub fn get_genesis_config(&self) -> anyhow::Result<GenesisConfig> {
        GenesisConfig::read(self.get_shell(), self.configs.join(GENESIS_FILE))
    }

    pub fn get_wallets_config(&self) -> anyhow::Result<WalletsConfig> {
        let path = self.configs.join(WALLETS_FILE);
        if let Ok(wallets) = WalletsConfig::read(self.get_shell(), &path) {
            return Ok(wallets);
        }
        if self.wallet_creation == WalletCreation::Localhost {
            let wallets = create_localhost_wallets(self.get_shell(), &self.link_to_code, self.id)?;
            wallets.save(self.get_shell(), &path)?;
            return Ok(wallets);
        }
        anyhow::bail!("Wallets configs has not been found");
    }
    pub fn get_contracts_config(&self) -> anyhow::Result<ContractsConfig> {
        ContractsConfig::read(self.get_shell(), self.configs.join(CONTRACTS_FILE))
    }

    pub fn path_to_foundry(&self) -> PathBuf {
        self.link_to_code.join(L1_CONTRACTS_FOUNDRY)
    }

    pub fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let config = self.get_internal();
        config.save(shell, path)
    }

    fn get_internal(&self) -> HyperchainConfigInternal {
        HyperchainConfigInternal {
            id: self.id,
            name: self.name.clone(),
            chain_id: self.chain_id,
            prover_version: self.prover_version,
            configs: self.configs.clone(),
            rocks_db_path: self.rocks_db_path.clone(),
            l1_batch_commit_data_generator_mode: self.l1_batch_commit_data_generator_mode,
            base_token: self.base_token.clone(),
            wallet_creation: self.wallet_creation,
        }
    }
}

impl ReadConfig for HyperchainConfigInternal {}
impl SaveConfig for HyperchainConfigInternal {}
