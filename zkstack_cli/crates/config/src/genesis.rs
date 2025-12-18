use std::path::Path;

use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_types::ProverMode;
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode, protocol_version::ProtocolSemanticVersion, L1ChainId,
    L2ChainId,
};

use crate::{
    raw::{PatchedConfig, RawConfig},
    ChainConfig,
};

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct L1VerifierConfig {
    // Rename is required to not introduce breaking changes in the API for existing clients.
    #[serde(
        alias = "recursion_scheduler_level_vk_hash",
        rename(serialize = "recursion_scheduler_level_vk_hash")
    )]
    pub snark_wrapper_vk_hash: String,
    pub fflonk_snark_wrapper_vk_hash: Option<String>,
}

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
        self.0.insert_yaml(
            "prover.dummy_verifier",
            config.prover_version == ProverMode::NoProofs,
        )?;
        Ok(())
    }

    pub fn update_from_contracts_genesis(
        &mut self,
        config: &ContractsGenesisConfig,
    ) -> anyhow::Result<()> {
        let l1_verifier: L1VerifierConfig = config.0.get("prover")?;

        let protocol_semantic_version = config.packed_protocol_semantic_version()?;
        self.0.insert(
            "genesis_protocol_semantic_version",
            ProtocolSemanticVersion::try_from_packed(protocol_semantic_version.into())
                .unwrap()
                .to_string(),
        )?;

        self.0.insert("genesis_root", config.genesis_root_hash()?)?;
        self.0.insert(
            "genesis_rollup_leaf_index",
            config.rollup_last_leaf_index()?,
        )?;
        self.0
            .insert("genesis_batch_commitment", config.genesis_commitment()?)?;
        self.0
            .insert("bootloader_hash", config.bootloader_hash()?)?;
        self.0
            .insert("default_aa_hash", config.default_aa_hash()?)?;
        if let Some(data) = config.evm_emulator_hash()? {
            self.0.insert("evm_emulator_hash", data)?;
        }
        self.0.insert(
            "prover.recursion_scheduler_level_vk_hash",
            l1_verifier.snark_wrapper_vk_hash,
        )?;
        if let Some(fflonk_vk_hash) = l1_verifier.fflonk_snark_wrapper_vk_hash {
            self.0
                .insert("prover.fflonk_snark_wrapper_vk_hash", fflonk_vk_hash)?;
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

    pub fn evm_emulator_hash(&self) -> anyhow::Result<Option<String>> {
        self.0.get_opt("evm_emulator_hash")
    }

    pub fn bootloader_hash(&self) -> anyhow::Result<String> {
        self.0.get("bootloader_hash")
    }
    pub fn default_aa_hash(&self) -> anyhow::Result<String> {
        self.0.get("default_aa_hash")
    }
    pub fn genesis_root_hash(&self) -> anyhow::Result<String> {
        self.0.get("genesis_root")
    }
    pub fn rollup_last_leaf_index(&self) -> anyhow::Result<u64> {
        self.0.get("genesis_rollup_leaf_index")
    }
    pub fn genesis_commitment(&self) -> anyhow::Result<String> {
        self.0.get("genesis_batch_commitment")
    }

    pub fn packed_protocol_semantic_version(&self) -> anyhow::Result<u64> {
        self.0.get::<u64>("protocol_semantic_version")
    }
}
