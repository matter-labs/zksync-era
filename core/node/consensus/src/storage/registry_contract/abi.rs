use zksync_basic_types::{
    ethabi::{Address, Bytes, Token},
    web3::contract::{Detokenize, Error, Tokenizable as _},
    U256,
};
use zksync_consensus_crypto::ByteFmt as _;
use zksync_consensus_roles::{attester};

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct Attester {
    pub weight: U256,
    pub node_owner: Address,
    pub pub_key: Bytes,
}

impl Attester {
    pub(crate) fn parse(&self) -> anyhow::Result<attester::WeightedAttester> {
        Ok(attester::WeightedAttester {
            key: attester::PublicKey::decode(&self.pub_key).context("key")?,
            weight: self.weight.try_into().context("weight overflow")?,
        })
    }
}

impl Detokenize for Attester {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        let mut tokens = tokens.into_iter();
        let next = |field| tokens.next().ok_or_else(|| Error::Other(format!("{field} missing")));
        Ok(Self {
            weight: U256::from_token(next("weight")?)?,
            node_owner: Address::from_token(next("node_owner")?)?,
            pub_key: Bytes::from_token(next("pub_key")?)?,
        })
    }
}
