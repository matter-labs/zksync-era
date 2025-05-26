use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize, Serializer};
use xshell::Shell;
use zkstack_cli_types::{BaseToken, L1BatchCommitmentMode, L1Network, ProverMode, WalletCreation};
use zksync_basic_types::L2ChainId;

use crate::{
    consts::{
        CONFIG_NAME, CONTRACTS_FILE, EN_CONFIG_FILE, GENERAL_FILE, GENESIS_FILE,
        L1_CONTRACTS_FOUNDRY, SECRETS_FILE, WALLETS_FILE,
    },
    create_localhost_wallets,
    gateway::GatewayConfig,
    traits::{
        FileConfigWithDefaultName, ReadConfig, ReadConfigWithBasePath, SaveConfig,
        SaveConfigWithBasePath, ZkStackConfig,
    },
    ContractsConfig, GatewayChainConfig, GeneralConfig, GenesisConfig, SecretsConfig,
    WalletsConfig, GATEWAY_CHAIN_FILE,
};

/// Chain configuration file. This file is created in the chain
/// directory before network initialization.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChainConfigInternal {
    // The id of chain on this machine allows to easily setup multiple chains,
    // needs for local setups only
    pub id: u32,
    pub name: String,
    pub chain_id: L2ChainId,
    pub prover_version: ProverMode,
    pub configs: PathBuf,
    pub rocks_db_path: PathBuf,
    pub external_node_config_path: Option<PathBuf>,
    pub artifacts_path: Option<PathBuf>,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub base_token: BaseToken,
    pub wallet_creation: WalletCreation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_bridge: Option<bool>,
    #[serde(default)] // for backward compatibility
    pub evm_emulator: bool,
}

/// Chain configuration file. This file is created in the chain
/// directory before network initialization.
#[derive(Debug)]
pub struct ChainConfig {
    pub id: u32,
    pub name: String,
    pub chain_id: L2ChainId,
    pub prover_version: ProverMode,
    pub l1_network: L1Network,
    pub link_to_code: PathBuf,
    pub rocks_db_path: PathBuf,
    pub artifacts: PathBuf,
    pub configs: PathBuf,
    pub external_node_config_path: Option<PathBuf>,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub base_token: BaseToken,
    pub wallet_creation: WalletCreation,
    pub shell: OnceCell<Shell>,
    pub legacy_bridge: Option<bool>,
    pub evm_emulator: bool,
}

#[derive(Debug, Clone)]
pub enum DAValidatorType {
    Rollup = 0,
    NoDA = 1,
    Avail = 2,
    EigenDA = 3,
    EigenDAM1 = 4,
}

impl Serialize for ChainConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.get_internal().serialize(serializer)
    }
}

impl ChainConfig {
    pub(crate) fn get_shell(&self) -> &Shell {
        self.shell.get().expect("Not initialized")
    }

    pub async fn get_genesis_config(&self) -> anyhow::Result<GenesisConfig> {
        GenesisConfig::read(self.get_shell(), self.path_to_genesis_config()).await
    }

    pub async fn get_general_config(&self) -> anyhow::Result<GeneralConfig> {
        GeneralConfig::read(self.get_shell(), self.path_to_general_config()).await
    }

    pub fn get_wallets_config(&self) -> anyhow::Result<WalletsConfig> {
        let path = self.configs.join(WALLETS_FILE);
        if self.get_shell().path_exists(&path) {
            return WalletsConfig::read(self.get_shell(), &path);
        }
        if self.wallet_creation == WalletCreation::Localhost {
            let wallets = create_localhost_wallets(self.get_shell(), &self.link_to_code, self.id)?;
            wallets.save(self.get_shell(), &path)?;
            return Ok(wallets);
        }
        anyhow::bail!("Wallets configs has not been found");
    }

    pub fn get_contracts_config(&self) -> anyhow::Result<ContractsConfig> {
        ContractsConfig::read_with_base_path(self.get_shell(), &self.configs)
    }

    pub async fn get_secrets_config(&self) -> anyhow::Result<SecretsConfig> {
        SecretsConfig::read(self.get_shell(), self.path_to_secrets_config()).await
    }

    pub fn get_gateway_config(&self) -> anyhow::Result<GatewayConfig> {
        GatewayConfig::read_with_base_path(self.get_shell(), &self.configs)
    }

    pub async fn get_gateway_chain_config(&self) -> anyhow::Result<GatewayChainConfig> {
        GatewayChainConfig::read(self.get_shell(), self.path_to_gateway_chain_config()).await
    }

    pub fn path_to_general_config(&self) -> PathBuf {
        self.configs.join(GENERAL_FILE)
    }

    pub fn path_to_external_node_config(&self) -> PathBuf {
        self.configs.join(EN_CONFIG_FILE)
    }

    pub fn path_to_genesis_config(&self) -> PathBuf {
        self.configs.join(GENESIS_FILE)
    }

    pub fn path_to_contracts_config(&self) -> PathBuf {
        self.configs.join(CONTRACTS_FILE)
    }

    pub fn path_to_secrets_config(&self) -> PathBuf {
        self.configs.join(SECRETS_FILE)
    }

    pub fn path_to_gateway_chain_config(&self) -> PathBuf {
        self.configs.join(GATEWAY_CHAIN_FILE)
    }

    pub fn path_to_l1_foundry(&self) -> PathBuf {
        self.link_to_code.join(L1_CONTRACTS_FOUNDRY)
    }

    pub fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let config = self.get_internal();
        config.save(shell, path)
    }

    pub fn save_with_base_path(self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let config = self.get_internal();
        config.save_with_base_path(shell, path)
    }

    fn get_internal(&self) -> ChainConfigInternal {
        ChainConfigInternal {
            id: self.id,
            name: self.name.clone(),
            chain_id: self.chain_id,
            prover_version: self.prover_version,
            configs: self.configs.clone(),
            rocks_db_path: self.rocks_db_path.clone(),
            external_node_config_path: self.external_node_config_path.clone(),
            artifacts_path: Some(self.artifacts.clone()),
            l1_batch_commit_data_generator_mode: self.l1_batch_commit_data_generator_mode,
            base_token: self.base_token.clone(),
            wallet_creation: self.wallet_creation,
            legacy_bridge: self.legacy_bridge,
            evm_emulator: self.evm_emulator,
        }
    }
}

impl FileConfigWithDefaultName for ChainConfigInternal {
    const FILE_NAME: &'static str = CONFIG_NAME;
}

impl ZkStackConfig for ChainConfigInternal {}
