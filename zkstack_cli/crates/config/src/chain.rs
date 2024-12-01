use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize, Serializer};
use types::{BaseToken, L1BatchCommitmentMode, L1Network, ProverMode, WalletCreation};
use xshell::Shell;
use zksync_basic_types::L2ChainId;

use crate::{
    consts::{
        CONFIG_NAME, CONTRACTS_FILE, EN_CONFIG_FILE, GENERAL_FILE, GENESIS_FILE,
        L1_CONTRACTS_FOUNDRY, SECRETS_FILE, WALLETS_FILE,
    },
    create_localhost_wallets,
    traits::{
        FileConfigWithDefaultName, ReadConfig, ReadConfigWithBasePath, SaveConfig,
        SaveConfigWithBasePath, ZkStackConfig,
    },
    utils::{find_file, get_preexisting_ecosystem_contracts_path},
    ContractsConfig, EcosystemConfig, GeneralConfig, GenesisConfig, SecretsConfig, WalletsConfig,
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
    pub l1_network: Option<L1Network>,
    pub link_to_code: Option<PathBuf>,
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

    pub fn get_genesis_config(&self) -> anyhow::Result<GenesisConfig> {
        GenesisConfig::read_with_base_path(self.get_shell(), &self.configs)
    }

    pub fn get_general_config(&self) -> anyhow::Result<GeneralConfig> {
        GeneralConfig::read_with_base_path(self.get_shell(), &self.configs)
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

    pub fn get_secrets_config(&self) -> anyhow::Result<SecretsConfig> {
        SecretsConfig::read_with_base_path(self.get_shell(), &self.configs)
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

    pub fn save_general_config(&self, general_config: &GeneralConfig) -> anyhow::Result<()> {
        general_config.save_with_base_path(self.get_shell(), &self.configs)
    }

    pub fn path_to_foundry(&self) -> PathBuf {
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

    pub fn get_preexisting_ecosystem_contracts_path(&self) -> PathBuf {
        get_preexisting_ecosystem_contracts_path(&self.link_to_code, self.l1_network)
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
            l1_network: Some(self.l1_network),
            link_to_code: Some(self.link_to_code.clone()),
            l1_batch_commit_data_generator_mode: self.l1_batch_commit_data_generator_mode,
            base_token: self.base_token.clone(),
            wallet_creation: self.wallet_creation,
            legacy_bridge: self.legacy_bridge,
            evm_emulator: self.evm_emulator,
        }
    }

    pub(crate) fn from_internal(
        chain_internal: ChainConfigInternal,
        shell: Shell,
    ) -> anyhow::Result<Self> {
        let l1_network = chain_internal.l1_network.context("L1 Network not found")?;
        let link_to_code = chain_internal
            .link_to_code
            .context("Link to code not found")?;
        let artifacts = chain_internal
            .artifacts_path
            .context("Artifacts path not found")?;

        Ok(Self {
            id: chain_internal.id,
            name: chain_internal.name,
            chain_id: chain_internal.chain_id,
            prover_version: chain_internal.prover_version,
            configs: chain_internal.configs,
            rocks_db_path: chain_internal.rocks_db_path,
            external_node_config_path: chain_internal.external_node_config_path,
            l1_network,
            l1_batch_commit_data_generator_mode: chain_internal.l1_batch_commit_data_generator_mode,
            base_token: chain_internal.base_token,
            wallet_creation: chain_internal.wallet_creation,
            legacy_bridge: chain_internal.legacy_bridge,
            link_to_code,
            artifacts,
            evm_emulator: chain_internal.evm_emulator,
            shell: shell.into(),
        })
    }
}

impl ChainConfigInternal {
    pub(crate) fn from_file(shell: &Shell) -> anyhow::Result<ChainConfigInternal> {
        let Ok(path) = find_file(shell, shell.current_dir(), CONFIG_NAME) else {
            bail!("Chain config not found")
        };

        shell.change_dir(&path);

        match ChainConfigInternal::read(shell, CONFIG_NAME) {
            Ok(config) => Ok(config),
            Err(err) => {
                if let Ok(ecosystem) = EcosystemConfig::read(shell, CONFIG_NAME) {
                    let chain = ecosystem.load_current_chain()?;
                    Ok(chain.get_internal())
                } else {
                    Err(err)
                }
            }
        }
    }
}

impl FileConfigWithDefaultName for ChainConfigInternal {
    const FILE_NAME: &'static str = CONFIG_NAME;
}

impl ZkStackConfig for ChainConfigInternal {}
