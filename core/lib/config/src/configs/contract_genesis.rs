use std::{fs, io::Write, path::Path};

use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zksync_basic_types::{
    protocol_version::{
        L1VerifierConfig, ProtocolSemanticVersion, ProtocolVersionId, VersionPatch,
    },
    H256,
};

/// Helper struct for deserializing protocol version from JSON object format
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct ProtocolVersionSerializerHelper {
    major: u32,
    minor: u32,
    patch: u32,
}

/// Custom serialization for ProtocolSemanticVersion to JSON object format
fn serialize_protocol_version<S>(
    version: &ProtocolSemanticVersion,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let json = ProtocolVersionSerializerHelper {
        major: 0,
        minor: version.minor as u32,
        patch: version.patch.0,
    };
    json.serialize(serializer)
}

/// Custom deserialization for ProtocolSemanticVersion from JSON object format
fn deserialize_protocol_version<'de, D>(
    deserializer: D,
) -> Result<ProtocolSemanticVersion, D::Error>
where
    D: Deserializer<'de>,
{
    let json = ProtocolVersionSerializerHelper::deserialize(deserializer)?;
    if json.major != 0 {
        return Err(serde::de::Error::custom("major version must be 0"));
    }
    let minor = ProtocolVersionId::try_from(json.minor as u16)
        .map_err(|_| serde::de::Error::custom("invalid minor version"))?;
    Ok(ProtocolSemanticVersion::new(
        minor,
        VersionPatch(json.patch),
    ))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContractsGenesis {
    #[serde(
        serialize_with = "serialize_protocol_version",
        deserialize_with = "deserialize_protocol_version"
    )]
    pub protocol_semantic_version: ProtocolSemanticVersion,
    pub genesis_root: H256,
    pub genesis_rollup_leaf_index: u64,
    pub genesis_batch_commitment: H256,
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub evm_emulator_hash: Option<H256>,
    pub prover: L1VerifierConfig,
}

impl ContractsGenesis {
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
