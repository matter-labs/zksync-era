use std::path::PathBuf;

use xshell::Shell;
use zksync_basic_types::{commitment::L1BatchCommitmentMode, L1ChainId, L2ChainId, SLChainId};

use crate::raw::PatchedConfig;

#[derive(Debug)]
pub struct ExternalNodeConfigPatch(PatchedConfig);

impl ExternalNodeConfigPatch {
    pub fn empty(shell: &Shell, path: PathBuf) -> Self {
        Self(PatchedConfig::empty(shell, path))
    }

    pub fn set_chain_ids(
        &mut self,
        l1: L1ChainId,
        l2: L2ChainId,
        gateway: Option<SLChainId>,
    ) -> anyhow::Result<()> {
        self.0.insert("l2_chain_id", l2.as_u64())?;
        self.0.insert("l1_chain_id", l1.0)?;
        if let Some(gateway_chain_id) = gateway {
            self.0.insert("gateway_chain_id", gateway_chain_id.0)?;
        }
        Ok(())
    }

    pub fn set_batch_commitment_mode(&mut self, mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
        self.0
            .insert_yaml("l1_batch_commit_data_generator_mode", mode)
    }

    pub fn set_main_node_url(&mut self, url: &str) -> anyhow::Result<()> {
        self.0.insert("main_node_url", url)
    }

    pub async fn save(self) -> anyhow::Result<()> {
        self.0.save().await
    }
}
