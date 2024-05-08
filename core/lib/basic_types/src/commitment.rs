use serde::{Deserialize, Serialize};
use web3::contract::{tokens::Detokenize, Error as Web3ContractError};

use crate::{ethabi, U256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum L1BatchCommitMode {
    #[default]
    Rollup,
    Validium,
}

// The cases are extracted from the `PubdataPricingMode` enum in the L1 contracts,
// And knowing that, in Ethereum, the response is the index of the enum case.
// 0 corresponds to Rollup case,
// 1 corresponds to Validium case,
// Other values are incorrect.
impl Detokenize for L1BatchCommitMode {
    fn from_tokens(tokens: Vec<ethabi::Token>) -> Result<Self, Web3ContractError> {
        fn error(tokens: &[ethabi::Token]) -> Web3ContractError {
            Web3ContractError::Api(web3::Error::Decoder(format!(
                "L1BatchCommitDataGeneratorMode::from_tokens: {tokens:?}"
            )))
        }

        match tokens.as_slice() {
            [ethabi::Token::Uint(enum_value)] => {
                if enum_value == &U256::zero() {
                    Ok(L1BatchCommitMode::Rollup)
                } else if enum_value == &U256::one() {
                    Ok(L1BatchCommitMode::Validium)
                } else {
                    Err(error(&tokens))
                }
            }
            _ => Err(error(&tokens)),
        }
    }
}
