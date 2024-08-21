use zksync_basic_types::{
    ethabi::{Address, Bytes, Token},
    web3::contract::{Detokenize, Error, Tokenizable as _},
    U256,
};
use zksync_consensus_crypto::ByteFmt as _;
use zksync_consensus_roles::{attester};

impl Attester {
    pub(crate) fn parse(&self) -> anyhow::Result<attester::WeightedAttester> {
        Ok(attester::WeightedAttester {
            key: attester::PublicKey::decode(&self.pub_key).context("key")?,
            weight: self.weight.try_into().context("weight overflow")?,
        })
    }
}

