use std::str::FromStr;

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

use crate::{
    ethabi,
    web3::contract::{Detokenize, Error as ContractError},
    Address, U256,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumIter, Display)]
pub enum L1BatchCommitmentMode {
    #[default]
    Rollup,
    Validium,
}

impl From<PubdataType> for L1BatchCommitmentMode {
    fn from(value: PubdataType) -> Self {
        match value {
            PubdataType::Rollup => L1BatchCommitmentMode::Rollup,
            PubdataType::NoDA
            | PubdataType::Avail
            | PubdataType::Celestia
            | PubdataType::Eigen
            | PubdataType::ObjectStore => L1BatchCommitmentMode::Validium,
        }
    }
}

// The cases are extracted from the `PubdataPricingMode` enum in the L1 contracts,
// And knowing that, in Ethereum, the response is the index of the enum case.
// 0 corresponds to Rollup case,
// 1 corresponds to Validium case,
// Other values are incorrect.
impl Detokenize for L1BatchCommitmentMode {
    fn from_tokens(tokens: Vec<ethabi::Token>) -> Result<Self, ContractError> {
        fn error(tokens: &[ethabi::Token]) -> ContractError {
            ContractError::InvalidOutputType(format!(
                "L1BatchCommitDataGeneratorMode::from_tokens: {tokens:?}"
            ))
        }

        match tokens.as_slice() {
            [ethabi::Token::Uint(enum_value)] => {
                if enum_value == &U256::zero() {
                    Ok(L1BatchCommitmentMode::Rollup)
                } else if enum_value == &U256::one() {
                    Ok(L1BatchCommitmentMode::Validium)
                } else {
                    Err(error(&tokens))
                }
            }
            _ => Err(error(&tokens)),
        }
    }
}

impl FromStr for L1BatchCommitmentMode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Rollup" => Ok(Self::Rollup),
            "Validium" => Ok(Self::Validium),
            _ => {
                Err("Incorrect l1 batch commitment mode type; expected one of `Rollup`, `Validium`")
            }
        }
    }
}

#[derive(Default, Copy, Debug, Clone, PartialEq, Serialize, Deserialize, Display)]
pub enum PubdataType {
    #[default]
    Rollup,
    NoDA,
    Avail,
    Celestia,
    Eigen,
    ObjectStore,
}

impl FromStr for PubdataType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Rollup" => Ok(Self::Rollup),
            "NoDA" => Ok(Self::NoDA),
            "Avail" => Ok(Self::Avail),
            "Celestia" => Ok(Self::Celestia),
            "Eigen" => Ok(Self::Eigen),
            "ObjectStore" => Ok(Self::ObjectStore),
            _ => Err("Incorrect DA client type; expected one of `Rollup`, `NoDA`, `Avail`, `Celestia`, `Eigen`, `ObjectStore`"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
#[repr(u8)]
pub enum L2DACommitmentScheme {
    None = 0,
    EmptyNoDA = 1,
    PubdataKeccak256 = 2,
    BlobsAndPubdataKeccak256 = 3,
}

impl L2DACommitmentScheme {
    pub fn is_none(&self) -> bool {
        *self == L2DACommitmentScheme::None
    }
}

impl TryFrom<u8> for L2DACommitmentScheme {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(L2DACommitmentScheme::None),
            1 => Ok(L2DACommitmentScheme::EmptyNoDA),
            2 => Ok(L2DACommitmentScheme::PubdataKeccak256),
            3 => Ok(L2DACommitmentScheme::BlobsAndPubdataKeccak256),
            _ => Err("Invalid L2DACommitmentScheme value"),
        }
    }
}

impl FromStr for L2DACommitmentScheme {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "None" => Ok(Self::None),
            "EmptyNoDA" => Ok(Self::EmptyNoDA),
            "PubdataKeccak256" => Ok(Self::PubdataKeccak256),
            "BlobsAndPubdataKeccak256" => Ok(Self::BlobsAndPubdataKeccak256),
            _ => Err("Incorrect L2 DA commitment scheme; expected one of `None`, `EmptyNoDA`, `PubdataKeccak256`, `BlobsAndPubdataKeccak256`"),
        }
    }
}

#[derive(Copy, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum L2PubdataValidator {
    Address(Address),
    CommitmentScheme(L2DACommitmentScheme),
}

impl TryFrom<(Option<Address>, Option<L2DACommitmentScheme>)> for L2PubdataValidator {
    type Error = anyhow::Error;

    fn try_from(
        value: (Option<Address>, Option<L2DACommitmentScheme>),
    ) -> Result<Self, Self::Error> {
        match value {
            (None, Some(scheme)) => Ok(L2PubdataValidator::CommitmentScheme(scheme)),
            (Some(address), None) => Ok(L2PubdataValidator::Address(address)),
            (Some(_), Some(_)) => anyhow::bail!(
                "Address and L2DACommitmentScheme are specified, should be chosen only one"
            ),
            (None, None) => anyhow::bail!(
                "Address and L2DACommitmentScheme are not specified, should be chosen at least one"
            ),
        }
    }
}

impl L2PubdataValidator {
    pub fn l2_da_validator(&self) -> Option<Address> {
        match self {
            L2PubdataValidator::Address(addr) => Some(*addr),
            L2PubdataValidator::CommitmentScheme(_) => None,
        }
    }

    pub fn l2_da_commitment_scheme(&self) -> Option<L2DACommitmentScheme> {
        match self {
            L2PubdataValidator::Address(_) => None,
            L2PubdataValidator::CommitmentScheme(scheme) => Some(*scheme),
        }
    }
}

#[derive(Copy, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PubdataParams {
    pubdata_validator: L2PubdataValidator,
    pubdata_type: PubdataType,
}

impl PubdataParams {
    pub fn new(
        pubdata_validator: L2PubdataValidator,
        pubdata_type: PubdataType,
    ) -> anyhow::Result<Self> {
        if L2PubdataValidator::CommitmentScheme(L2DACommitmentScheme::None) == pubdata_validator {
            anyhow::bail!("L2DACommitmentScheme::None is not allowed as a legit pubdata parameter");
        };

        Ok(PubdataParams {
            pubdata_validator,
            pubdata_type,
        })
    }

    pub fn pubdata_validator(&self) -> L2PubdataValidator {
        self.pubdata_validator
    }

    pub fn pubdata_type(&self) -> PubdataType {
        self.pubdata_type
    }

    pub fn genesis() -> Self {
        PubdataParams {
            pubdata_validator: L2PubdataValidator::CommitmentScheme(
                L2DACommitmentScheme::BlobsAndPubdataKeccak256,
            ),
            pubdata_type: PubdataType::Rollup,
        }
    }

    pub fn pre_gateway() -> Self {
        PubdataParams {
            pubdata_validator: L2PubdataValidator::Address(Address::zero()),
            pubdata_type: Default::default(),
        }
    }
}
