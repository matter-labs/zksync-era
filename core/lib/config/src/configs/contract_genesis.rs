use std::{fs, io::Write, path::Path};

use anyhow::Context;
use zksync_basic_types::{
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion, ProtocolVersionId},
    H256,
};

pub const DEFAULT_GENESIS_FILE_PATH: &str = "../contracts/configs/genesis/era/latest.json";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpecializedProtocolSemanticVersionForContracts {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl From<SpecializedProtocolSemanticVersionForContracts> for ProtocolSemanticVersion {
    fn from(version: SpecializedProtocolSemanticVersionForContracts) -> Self {
        ProtocolSemanticVersion::new(
            ProtocolVersionId::try_from(version.minor as u16).unwrap(),
            zksync_basic_types::protocol_version::VersionPatch(version.patch),
        )
    }
}

impl From<ProtocolSemanticVersion> for SpecializedProtocolSemanticVersionForContracts {
    fn from(version: ProtocolSemanticVersion) -> Self {
        Self {
            major: 0,
            minor: version.minor as u32,
            patch: version.patch.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContractsGenesis {
    pub protocol_semantic_version: SpecializedProtocolSemanticVersionForContracts,
    pub genesis_root: H256,
    pub genesis_rollup_leaf_index: u64,
    pub genesis_batch_commitment: H256,
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub evm_emulator_hash: Option<H256>,
    pub prover: L1VerifierConfig,
}

impl ContractsGenesis {
    pub fn protocol_semantic_version(&self) -> ProtocolSemanticVersion {
        self.protocol_semantic_version.clone().into()
    }

    /// **Important:** This method uses blocking I/O.
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(
            &fs::read_to_string(path)
                .with_context(|| format!("failed reading genesis config file at {:?}", path))?,
        )?)
    }

    /// **Important:** This method uses blocking I/O.
    pub fn write(self, path: &Path) -> anyhow::Result<()> {
        let path = path.to_owned();
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed creating genesis config file at {:?}", path))?;
        let string = serde_json::to_string_pretty(&self)
            .context("failed serializing config to json string")?;
        file.write_all(string.as_bytes())?;
        Ok(())
    }
}
