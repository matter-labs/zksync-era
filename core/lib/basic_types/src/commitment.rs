use std::str::FromStr;

use ethabi::Address;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

use crate::{
    ethabi,
    web3::contract::{Detokenize, Error as ContractError},
    U256,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumIter, Display)]
pub enum L1BatchCommitmentMode {
    #[default]
    Rollup,
    Validium,
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

#[derive(Default, Copy, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PubdataParams {
    pub l2_da_validator_address: Address,
    // TOODO: maybe rename / use new type.
    pub pubdata_type: L1BatchCommitmentMode,
}
