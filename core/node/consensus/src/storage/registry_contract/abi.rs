use zksync_basic_types::{
    ethabi::{Address, Bytes, Token},
    web3::contract::{Detokenize, Error, Tokenizable as _},
    H160, U256,
};
use zksync_consensus_crypto::ByteFmt as _;
use zksync_consensus_roles::{attester, validator};
use zksync_dal::consensus_dal;

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CommitteeValidator {
    pub node_owner: Address,
    pub weight: usize,
    pub pub_key: Vec<u8>,
    pub pop: Vec<u8>,
}

impl Detokenize for CommitteeValidator {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        Ok(Self {
            node_owner: H160::from_token(
                tokens
                    .get(0)
                    .ok_or_else(|| Error::Other("tokens[0] missing".to_string()))?
                    .clone(),
            )?,
            weight: U256::from_token(
                tokens
                    .get(1)
                    .ok_or_else(|| Error::Other("tokens[1] missing".to_string()))?
                    .clone(),
            )?
            .as_usize(),
            pub_key: Bytes::from_token(
                tokens
                    .get(2)
                    .ok_or_else(|| Error::Other("tokens[2] missing".to_string()))?
                    .clone(),
            )?,
            pop: Bytes::from_token(
                tokens
                    .get(3)
                    .ok_or_else(|| Error::Other("tokens[3] missing".to_string()))?
                    .clone(),
            )?,
        })
    }
}

impl TryInto<consensus_dal::Validator> for CommitteeValidator {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<consensus_dal::Validator, Self::Error> {
        Ok(consensus_dal::Validator {
            pub_key: validator::PublicKey::decode(&self.pub_key).unwrap(),
            weight: self.weight as u64,
            proof_of_possession: validator::Signature::decode(&self.pop).unwrap(),
        })
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CommitteeAttester {
    pub weight: usize,
    pub node_owner: Address,
    pub pub_key: Vec<u8>,
}

impl Detokenize for CommitteeAttester {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        Ok(Self {
            weight: U256::from_token(
                tokens
                    .get(0)
                    .ok_or_else(|| Error::Other("tokens[0] missing".to_string()))?
                    .clone(),
            )?
            .as_usize(),
            node_owner: H160::from_token(
                tokens
                    .get(1)
                    .ok_or_else(|| Error::Other("tokens[1] missing".to_string()))?
                    .clone(),
            )?,
            pub_key: Bytes::from_token(
                tokens
                    .get(2)
                    .ok_or_else(|| Error::Other("tokens[2] missing".to_string()))?
                    .clone(),
            )?,
        })
    }
}

impl TryInto<consensus_dal::Attester> for CommitteeAttester {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<consensus_dal::Attester, Self::Error> {
        Ok(consensus_dal::Attester {
            pub_key: attester::PublicKey::decode(&self.pub_key).unwrap(),
            weight: self.weight as u64,
        })
    }
}
