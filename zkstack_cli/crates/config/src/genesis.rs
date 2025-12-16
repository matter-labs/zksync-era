use std::path::Path;

use xshell::Shell;
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode, protocol_version::ProtocolSemanticVersion, L1ChainId,
    L2ChainId, H256,
};

use crate::{
    raw::{PatchedConfig, RawConfig},
    ChainConfig,
};

#[derive(Debug)]
pub struct GenesisConfig(pub(crate) RawConfig);

// TODO get rid of the methods. Genesis config now should be used only for getting root data
impl GenesisConfig {
    pub async fn read(shell: &Shell, path: &Path) -> anyhow::Result<Self> {
        RawConfig::read(shell, path).await.map(Self)
    }

    pub fn l1_chain_id(&self) -> anyhow::Result<L1ChainId> {
        self.0.get("l1_chain_id")
    }

    pub fn l2_chain_id(&self) -> anyhow::Result<L2ChainId> {
        self.0.get("l2_chain_id")
    }

    pub fn l1_batch_commitment_mode(&self) -> anyhow::Result<L1BatchCommitmentMode> {
        self.0.get("l1_batch_commit_data_generator_mode")
    }

    pub fn patched(self) -> GenesisConfigPatch {
        GenesisConfigPatch(self.0.patched())
    }
}

#[derive(Debug)]
pub struct GenesisConfigPatch(PatchedConfig);

impl GenesisConfigPatch {
    pub fn update_from_chain_config(&mut self, config: &ChainConfig) -> anyhow::Result<()> {
        self.0.insert("l2_chain_id", config.chain_id.as_u64())?;
        // TODO(EVM-676): for now, the settlement layer is always the same as the L1 network
        self.0.insert("l1_chain_id", config.l1_network.chain_id())?;
        self.0.insert_yaml(
            "l1_batch_commit_data_generator_mode",
            config.l1_batch_commit_data_generator_mode,
        )?;
        Ok(())
    }

    pub fn update_from_contracts_genesis(
        &mut self,
        config: &ContractsGenesisConfig,
    ) -> anyhow::Result<()> {
        let protocol_semantic_version = config.0.get::<u64>("protocol_semantic_version")?;
        self.0.insert(
            "genesis_protocol_semantic_version",
            ProtocolSemanticVersion::try_from_packed(protocol_semantic_version.into())
                .unwrap()
                .to_string(),
        )?;

        self.0
            .insert("genesis_root", config.0.get::<String>("genesis_root")?)?;
        self.0.insert(
            "genesis_rollup_leaf_index",
            config.0.get::<u64>("genesis_rollup_leaf_index")?,
        )?;
        self.0.insert(
            "genesis_batch_commitment",
            config.0.get::<String>("genesis_batch_commitment")?,
        )?;
        self.0.insert(
            "bootloader_hash",
            config.0.get::<String>("bootloader_hash")?,
        )?;
        self.0.insert(
            "default_aa_hash",
            config.0.get::<String>("default_aa_hash")?,
        )?;
        if let Some(data) = config.0.get_opt::<String>("evm_emulator_hash")? {
            self.0.insert("evm_emulator_hash", data)?;
        }

        Ok(())
    }

    pub async fn save(self) -> anyhow::Result<()> {
        self.0.save().await
    }
}

#[derive(Debug)]
pub struct ContractsGenesisConfig(pub(crate) RawConfig);
impl ContractsGenesisConfig {
    pub async fn read(shell: &Shell, path: &Path) -> anyhow::Result<Self> {
        RawConfig::read(shell, path).await.map(Self)
    }

    pub fn evm_emulator_hash(&self) -> anyhow::Result<Option<H256>> {
        self.0.get_opt("evm_emulator_hash")
    }
}
