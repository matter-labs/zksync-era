use std::{fs, path::Path};

use anyhow::Context;
use zksync_basic_types::{protocol_version::ProtocolSemanticVersion, H256};

pub const DEFAULT_GENESIS_FILE_PATH: &str = "../contracts/configs/genesis/era/latest.toml";

#[derive(Debug, Clone, PartialOrd, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContractsGenesis {
    pub protocol_semantic_version: u64,
    pub genesis_root: H256,
    pub genesis_rollup_leaf_index: u64,
    pub genesis_batch_commitment: H256,
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub evm_emulator_hash: Option<H256>,
}

impl ContractsGenesis {
    pub fn protocol_semantic_version(&self) -> ProtocolSemanticVersion {
        ProtocolSemanticVersion::try_from_packed(self.protocol_semantic_version.into()).unwrap()
    }
    /// **Important:** This method uses blocking I/O.
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        Ok(toml::from_str(&fs::read_to_string(&path).with_context(
            || format!("failed reading genesis config file at {:?}", path),
        )?)?)
    }

    /// **Important:** This method uses blocking I/O.
    pub fn write(self, path: &Path) -> anyhow::Result<()> {
        let path = path.to_owned();
        let file = fs::File::create(&path)
            .with_context(|| format!("failed creating genesis config file at {:?}", path))?;
        let string = toml::to_string(&self).context("failed serializing config to YAML string")?;
        Ok(fs::write(&path, string)?)
    }
}
