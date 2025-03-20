use std::path::PathBuf;

use xshell::Shell;
use zksync_basic_types::{commitment::L1BatchCommitmentMode, L1ChainId, L2ChainId, H256};

use crate::{
    raw::{PatchedConfig, RawConfig},
    ChainConfig,
};

#[derive(Debug)]
pub struct GenesisConfig(pub(crate) RawConfig);

impl GenesisConfig {
    pub async fn read(shell: &Shell, path: PathBuf) -> anyhow::Result<Self> {
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

    pub fn evm_emulator_hash(&self) -> anyhow::Result<Option<H256>> {
        self.0.get_opt("evm_emulator_hash")
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

    pub async fn save(self) -> anyhow::Result<()> {
        self.0.save().await
    }
}
