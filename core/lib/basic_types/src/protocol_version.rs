use std::{
    convert::{TryFrom, TryInto},
    fmt,
    num::ParseIntError,
    ops::{Add, Deref, DerefMut, Sub},
    str::FromStr,
};

use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

use crate::{
    ethabi::Token,
    vm::VmVersion,
    web3::contract::{Detokenize, Error},
    H256, U256,
};

pub const PACKED_SEMVER_MINOR_OFFSET: u32 = 32;
pub const PACKED_SEMVER_MINOR_MASK: u32 = 0xFFFF;

/// `ProtocolVersionId` is a unique identifier of the protocol version.
///
/// Note, that it is an identifier of the `minor` semver version of the protocol, with
/// the `major` version being `0`. Also, the protocol version on the contracts may contain
/// potential minor versions, that may have different contract behavior (e.g. Verifier), but it should not
/// impact the users.
#[repr(u16)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    TryFromPrimitive,
    Serialize,
    Deserialize,
)]
pub enum ProtocolVersionId {
    Version0 = 0,
    Version1,
    Version2,
    Version3,
    Version4,
    Version5,
    Version6,
    Version7,
    Version8,
    Version9,
    Version10,
    Version11,
    Version12,
    Version13,
    Version14,
    Version15,
    Version16,
    Version17,
    Version18,
    Version19,
    Version20,
    Version21,
    Version22,
    // Version `23` is only present on the internal staging networks.
    // All the user-facing environments were switched from 22 to 24 right away.
    Version23,
    Version24,
    Version25,
    Version26,
    Version27,
    Version28,
    Version29,
}

impl ProtocolVersionId {
    pub const fn latest() -> Self {
        Self::Version28
    }

    pub const fn next() -> Self {
        Self::Version29
    }

    pub fn try_from_packed_semver(packed_semver: U256) -> Result<Self, String> {
        ProtocolSemanticVersion::try_from_packed(packed_semver).map(|p| p.minor)
    }

    pub fn into_packed_semver_with_patch(self, patch: usize) -> U256 {
        let minor = U256::from(self as u16);
        let patch = U256::from(patch as u32);

        (minor << U256::from(PACKED_SEMVER_MINOR_OFFSET)) | patch
    }

    /// Returns VM version to be used by API for this protocol version.
    /// We temporary support only two latest VM versions for API.
    pub fn into_api_vm_version(self) -> VmVersion {
        match self {
            ProtocolVersionId::Version0 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version1 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version2 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version3 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version4 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version5 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version6 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version7 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version8 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version9 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version10 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version11 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version12 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version13 => VmVersion::VmVirtualBlocks,
            ProtocolVersionId::Version14 => VmVersion::VmVirtualBlocks,
            ProtocolVersionId::Version15 => VmVersion::VmVirtualBlocks,
            ProtocolVersionId::Version16 => VmVersion::VmVirtualBlocksRefundsEnhancement,
            ProtocolVersionId::Version17 => VmVersion::VmVirtualBlocksRefundsEnhancement,
            ProtocolVersionId::Version18 => VmVersion::VmBoojumIntegration,
            ProtocolVersionId::Version19 => VmVersion::VmBoojumIntegration,
            ProtocolVersionId::Version20 => VmVersion::Vm1_4_1,
            ProtocolVersionId::Version21 => VmVersion::Vm1_4_2,
            ProtocolVersionId::Version22 => VmVersion::Vm1_4_2,
            ProtocolVersionId::Version23 => VmVersion::Vm1_5_0SmallBootloaderMemory,
            ProtocolVersionId::Version24 => VmVersion::Vm1_5_0IncreasedBootloaderMemory,
            ProtocolVersionId::Version25 => VmVersion::Vm1_5_0IncreasedBootloaderMemory,
            ProtocolVersionId::Version26 => VmVersion::VmGateway,
            ProtocolVersionId::Version27 => VmVersion::VmEvmEmulator,
            ProtocolVersionId::Version28 => VmVersion::VmEcPrecompiles,
            // Speculative VM version for the next protocol version to be used in the upgrade integration test etc.
            ProtocolVersionId::Version29 => VmVersion::VmEcPrecompiles,
        }
    }

    // It is possible that some external nodes do not store protocol versions for versions below 9.
    // That's why we assume that whenever a protocol version is not present, version 9 is to be used.
    pub fn last_potentially_undefined() -> Self {
        Self::Version9
    }

    pub fn is_pre_boojum(&self) -> bool {
        self <= &Self::Version17
    }

    pub fn is_pre_shared_bridge(&self) -> bool {
        self <= &Self::Version22
    }

    pub fn is_pre_gateway(&self) -> bool {
        self < &Self::gateway_upgrade()
    }

    pub fn is_post_gateway(&self) -> bool {
        self >= &Self::gateway_upgrade()
    }

    pub fn is_pre_fflonk(&self) -> bool {
        self < &Self::Version27
    }

    pub fn is_post_fflonk(&self) -> bool {
        self >= &Self::Version27
    }

    pub fn is_1_4_0(&self) -> bool {
        self >= &ProtocolVersionId::Version18 && self < &ProtocolVersionId::Version20
    }

    pub fn is_1_4_1(&self) -> bool {
        self == &ProtocolVersionId::Version20
    }

    pub fn is_pre_1_4_1(&self) -> bool {
        self < &ProtocolVersionId::Version20
    }

    pub fn is_post_1_4_1(&self) -> bool {
        self >= &ProtocolVersionId::Version20
    }

    pub fn is_post_1_4_2(&self) -> bool {
        self >= &ProtocolVersionId::Version21
    }

    pub fn is_pre_1_4_2(&self) -> bool {
        self < &ProtocolVersionId::Version21
    }

    pub fn is_1_4_2(&self) -> bool {
        self == &ProtocolVersionId::Version21 || self == &ProtocolVersionId::Version22
    }

    pub fn is_pre_1_5_0(&self) -> bool {
        self < &ProtocolVersionId::Version23
    }

    pub fn is_post_1_5_0(&self) -> bool {
        self >= &ProtocolVersionId::Version23
    }

    pub const fn gateway_upgrade() -> Self {
        ProtocolVersionId::Version26
    }

    pub fn is_pre_29_interop(&self) -> bool {
        self < &Self::Version29
    }
}

impl Default for ProtocolVersionId {
    fn default() -> Self {
        Self::latest()
    }
}

impl fmt::Display for ProtocolVersionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", *self as u16)
    }
}

impl TryFrom<U256> for ProtocolVersionId {
    type Error = String;

    fn try_from(value: U256) -> Result<Self, Self::Error> {
        if value > U256::from(u16::MAX) {
            Err(format!("unknown protocol version ID: {}", value))
        } else {
            (value.as_u32() as u16)
                .try_into()
                .map_err(|_| format!("unknown protocol version ID: {}", value))
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerifierParams {
    pub recursion_node_level_vk_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_circuits_set_vks_hash: H256,
}

impl Detokenize for VerifierParams {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        if tokens.len() != 1 {
            return Err(Error::InvalidOutputType(format!(
                "expected single token, got {tokens:?}"
            )));
        }

        let tokens = match tokens[0].clone() {
            Token::Tuple(tokens) => tokens,
            other => {
                return Err(Error::InvalidOutputType(format!(
                    "expected a tuple, got {other:?}"
                )));
            }
        };

        let vks_vec: Vec<H256> = tokens
            .into_iter()
            .map(|token| H256::from_slice(&token.into_fixed_bytes().unwrap()))
            .collect();
        Ok(VerifierParams {
            recursion_node_level_vk_hash: vks_vec[0],
            recursion_leaf_level_vk_hash: vks_vec[1],
            recursion_circuits_set_vks_hash: vks_vec[2],
        })
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct L1VerifierConfig {
    // Rename is required to not introduce breaking changes in the API for existing clients.
    #[serde(
        alias = "recursion_scheduler_level_vk_hash",
        rename(serialize = "recursion_scheduler_level_vk_hash")
    )]
    pub snark_wrapper_vk_hash: H256,
    pub fflonk_snark_wrapper_vk_hash: Option<H256>,
}

impl From<ProtocolVersionId> for VmVersion {
    fn from(value: ProtocolVersionId) -> Self {
        match value {
            ProtocolVersionId::Version0 => VmVersion::M5WithoutRefunds,
            ProtocolVersionId::Version1 => VmVersion::M5WithoutRefunds,
            ProtocolVersionId::Version2 => VmVersion::M5WithRefunds,
            ProtocolVersionId::Version3 => VmVersion::M5WithRefunds,
            ProtocolVersionId::Version4 => VmVersion::M6Initial,
            ProtocolVersionId::Version5 => VmVersion::M6BugWithCompressionFixed,
            ProtocolVersionId::Version6 => VmVersion::M6BugWithCompressionFixed,
            ProtocolVersionId::Version7 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version8 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version9 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version10 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version11 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version12 => VmVersion::Vm1_3_2,
            ProtocolVersionId::Version13 => VmVersion::VmVirtualBlocks,
            ProtocolVersionId::Version14 => VmVersion::VmVirtualBlocks,
            ProtocolVersionId::Version15 => VmVersion::VmVirtualBlocks,
            ProtocolVersionId::Version16 => VmVersion::VmVirtualBlocksRefundsEnhancement,
            ProtocolVersionId::Version17 => VmVersion::VmVirtualBlocksRefundsEnhancement,
            ProtocolVersionId::Version18 => VmVersion::VmBoojumIntegration,
            ProtocolVersionId::Version19 => VmVersion::VmBoojumIntegration,
            ProtocolVersionId::Version20 => VmVersion::Vm1_4_1,
            ProtocolVersionId::Version21 => VmVersion::Vm1_4_2,
            ProtocolVersionId::Version22 => VmVersion::Vm1_4_2,
            ProtocolVersionId::Version23 => VmVersion::Vm1_5_0SmallBootloaderMemory,
            ProtocolVersionId::Version24 => VmVersion::Vm1_5_0IncreasedBootloaderMemory,
            ProtocolVersionId::Version25 => VmVersion::Vm1_5_0IncreasedBootloaderMemory,
            ProtocolVersionId::Version26 => VmVersion::VmGateway,
            ProtocolVersionId::Version27 => VmVersion::VmEvmEmulator,
            ProtocolVersionId::Version28 => VmVersion::VmEcPrecompiles,
            // Speculative VM version for the next protocol version to be used in the upgrade integration test etc.
            ProtocolVersionId::Version29 => VmVersion::VmEcPrecompiles,
        }
    }
}

basic_type!(
    /// Patch part of semantic protocol version.
    VersionPatch,
    u32
);

/// Semantic protocol version.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, SerializeDisplay, DeserializeFromStr, Hash, PartialOrd, Ord,
)]
pub struct ProtocolSemanticVersion {
    pub minor: ProtocolVersionId,
    pub patch: VersionPatch,
}

impl ProtocolSemanticVersion {
    const MAJOR_VERSION: u8 = 0;

    pub fn new(minor: ProtocolVersionId, patch: VersionPatch) -> Self {
        Self { minor, patch }
    }

    pub fn try_from_packed(packed: U256) -> Result<Self, String> {
        let minor = ((packed >> U256::from(PACKED_SEMVER_MINOR_OFFSET))
            & U256::from(PACKED_SEMVER_MINOR_MASK))
        .try_into()?;
        let patch = packed.0[0] as u32;

        Ok(Self {
            minor,
            patch: VersionPatch(patch),
        })
    }

    pub fn pack(&self) -> U256 {
        (U256::from(self.minor as u16) << U256::from(PACKED_SEMVER_MINOR_OFFSET))
            | U256::from(self.patch.0)
    }
}

impl fmt::Display for ProtocolSemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}",
            Self::MAJOR_VERSION,
            self.minor as u16,
            self.patch
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseProtocolSemanticVersionError {
    #[error("invalid format")]
    InvalidFormat,
    #[error("non zero major version")]
    NonZeroMajorVersion,
    #[error("{0}")]
    ParseIntError(ParseIntError),
}

impl FromStr for ProtocolSemanticVersion {
    type Err = ParseProtocolSemanticVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(ParseProtocolSemanticVersionError::InvalidFormat);
        }

        let major = parts[0]
            .parse::<u16>()
            .map_err(ParseProtocolSemanticVersionError::ParseIntError)?;
        if major != 0 {
            return Err(ParseProtocolSemanticVersionError::NonZeroMajorVersion);
        }

        let minor = parts[1]
            .parse::<u16>()
            .map_err(ParseProtocolSemanticVersionError::ParseIntError)?;
        let minor = ProtocolVersionId::try_from(minor)
            .map_err(|_| ParseProtocolSemanticVersionError::InvalidFormat)?;

        let patch = parts[2]
            .parse::<u32>()
            .map_err(ParseProtocolSemanticVersionError::ParseIntError)?;

        Ok(ProtocolSemanticVersion {
            minor,
            patch: patch.into(),
        })
    }
}

impl Default for ProtocolSemanticVersion {
    fn default() -> Self {
        Self {
            minor: Default::default(),
            patch: 0.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_packing() {
        let version = ProtocolSemanticVersion {
            minor: ProtocolVersionId::latest(),
            patch: 10.into(),
        };

        let packed = version.pack();
        let unpacked = ProtocolSemanticVersion::try_from_packed(packed).unwrap();

        assert_eq!(version, unpacked);
    }

    #[test]
    fn test_verifier_config_serde() {
        let de = [
            r#"{"recursion_scheduler_level_vk_hash": "0x1111111111111111111111111111111111111111111111111111111111111111"}"#,
            r#"{"snark_wrapper_vk_hash": "0x1111111111111111111111111111111111111111111111111111111111111111"}"#,
        ];
        for de in de.iter() {
            let _: L1VerifierConfig = serde_json::from_str(de)
                .unwrap_or_else(|err| panic!("Failed deserialization. String: {de}, error {err}"));
        }
        let ser = L1VerifierConfig {
            snark_wrapper_vk_hash: H256::repeat_byte(0x11),
            fflonk_snark_wrapper_vk_hash: Some(H256::repeat_byte(0x11)),
        };
        let ser_str = serde_json::to_string(&ser).unwrap();
        let expected_str = r#"{"recursion_scheduler_level_vk_hash":"0x1111111111111111111111111111111111111111111111111111111111111111","fflonk_snark_wrapper_vk_hash":"0x1111111111111111111111111111111111111111111111111111111111111111"}"#;
        assert_eq!(ser_str, expected_str);
    }
}
