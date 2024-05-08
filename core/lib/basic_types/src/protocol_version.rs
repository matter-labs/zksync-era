use std::convert::{TryFrom, TryInto};

use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};

use crate::{
    ethabi::Token,
    vm_version::VmVersion,
    web3::contract::{Detokenize, Error},
    H256, U256,
};

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
}

impl ProtocolVersionId {
    pub fn latest() -> Self {
        Self::Version24
    }

    pub fn current_prover_version() -> Self {
        Self::Version24
    }

    pub fn next() -> Self {
        Self::Version25
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
}

impl Default for ProtocolVersionId {
    fn default() -> Self {
        Self::latest()
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
                )))
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
    pub params: VerifierParams,
    pub recursion_scheduler_level_vk_hash: H256,
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
        }
    }
}
